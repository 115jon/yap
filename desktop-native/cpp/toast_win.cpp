#include "toast_win.h"

// The WinRT ABI through WRL, not C++/WinRT: the static Qt was built with
// another C++/WinRT version, and the linker refuses to mix two (their
// inline code would collide).

#include <cstdio>
#include <deque>
#include <mutex>

#include <roapi.h>
#include <windows.data.xml.dom.h>
#include <windows.foundation.collections.h>
#include <windows.ui.notifications.h>
#include <wrl/client.h>
#include <wrl/event.h>
#include <wrl/wrappers/corewrappers.h>

using ABI::Windows::Data::Xml::Dom::IXmlDocument;
using ABI::Windows::Data::Xml::Dom::IXmlDocumentIO;
using ABI::Windows::Foundation::IPropertyValue;
using ABI::Windows::Foundation::ITypedEventHandler;
using ABI::Windows::Foundation::Collections::IPropertySet;
using ABI::Windows::UI::Notifications::IToastActivatedEventArgs;
using ABI::Windows::UI::Notifications::IToastActivatedEventArgs2;
using ABI::Windows::UI::Notifications::IToastNotification;
using ABI::Windows::UI::Notifications::IToastNotification2;
using ABI::Windows::UI::Notifications::IToastNotificationFactory;
using ABI::Windows::UI::Notifications::IToastNotificationManagerStatics;
using ABI::Windows::UI::Notifications::IToastNotifier;
using ABI::Windows::UI::Notifications::ToastNotification;
using Microsoft::WRL::Callback;
using Microsoft::WRL::ComPtr;
using Microsoft::WRL::Wrappers::HString;
using Microsoft::WRL::Wrappers::HStringReference;
using StringMap = ABI::Windows::Foundation::Collections::IMap<HSTRING, IInspectable*>;

using ActivatedHandler = ITypedEventHandler<ToastNotification*, IInspectable*>;

namespace {

// Toasts stay alive (with their handler) while they may still be clicked;
// a toast object that is released no longer reports activation.
constexpr std::size_t kKept = 32;
std::mutex g_lock;
std::deque<ComPtr<IToastNotification>> g_kept;

std::wstring escape(const std::wstring& text)
{
  std::wstring out;
  out.reserve(text.size());
  for (wchar_t c : text) {
    switch (c) {
      case L'&': out += L"&amp;"; break;
      case L'<': out += L"&lt;"; break;
      case L'>': out += L"&gt;"; break;
      case L'"': out += L"&quot;"; break;
      case L'\'': out += L"&apos;"; break;
      default:
        // XML 1.0 allows no other control characters.
        if (c >= 0x20 || c == L'\t' || c == L'\n' || c == L'\r')
          out += c;
    }
  }
  return out;
}

// The reply box's input id, read back from the activation's user input.
constexpr wchar_t kReplyInput[] = L"reply";

std::wstring xml(const toast::Content& content)
{
  std::wstring out =
    L"<toast launch=\"action=open\"><visual><binding template=\"ToastGeneric\">";
  out += L"<text hint-maxLines=\"1\">" + escape(content.title) + L"</text>";
  if (!content.summary.empty())
    out += L"<text hint-maxLines=\"1\">" + escape(content.summary) + L"</text>";
  out += L"<text>" + escape(content.body) + L"</text>";
  out += L"</binding></visual>";
  if (content.messageActions) {
    // desktop-toasts.ts: Reply (a text box, "Send") and Mark as Read. The
    // activation is "foreground" (the in-process Activated event fires
    // for it); the handler decides whether the window shows.
    out += L"<actions>";
    out += L"<input id=\"";
    out += kReplyInput;
    out += L"\" type=\"text\" placeHolderContent=\"Type your reply...\"/>";
    out += L"<action content=\"Send\" arguments=\"action=reply\" "
           L"activationType=\"foreground\" hint-inputId=\"";
    out += kReplyInput;
    out += L"\"/>";
    out += L"<action content=\"Mark as Read\" arguments=\"action=mark-read\" "
           L"activationType=\"foreground\"/>";
    out += L"</actions>";
  }
  out += L"<audio silent=\"true\"/></toast>";
  return out;
}

std::wstring fromHstring(HSTRING value)
{
  UINT32 length = 0;
  const wchar_t* raw = WindowsGetStringRawBuffer(value, &length);
  return raw ? std::wstring(raw, length) : std::wstring();
}

// The text typed in the reply box, if any.
std::wstring replyText(IInspectable* args)
{
  ComPtr<IInspectable> inspectable(args);
  ComPtr<IToastActivatedEventArgs2> withInput;
  ComPtr<IPropertySet> input;
  ComPtr<StringMap> map;
  if (!args || FAILED(inspectable.As(&withInput)) ||
      FAILED(withInput->get_UserInput(&input)) || !input ||
      FAILED(input.As(&map)))
    return {};
  boolean has = false;
  HStringReference key(kReplyInput);
  if (FAILED(map->HasKey(key.Get(), &has)) || !has)
    return {};
  ComPtr<IInspectable> value;
  ComPtr<IPropertyValue> property;
  HString text;
  if (FAILED(map->Lookup(key.Get(), &value)) || !value ||
      FAILED(value.As(&property)) ||
      FAILED(property->GetString(text.GetAddressOf())))
    return {};
  return fromHstring(text.Get());
}

std::wstring activationArguments(IInspectable* args)
{
  ComPtr<IInspectable> inspectable(args);
  ComPtr<IToastActivatedEventArgs> activated;
  HString arguments;
  if (!args || FAILED(inspectable.As(&activated)) ||
      FAILED(activated->get_Arguments(arguments.GetAddressOf())))
    return {};
  return fromHstring(arguments.Get());
}

bool failed(HRESULT hr, const char* step)
{
  if (SUCCEEDED(hr))
    return false;
  std::fprintf(stderr, "[toast] %s failed: 0x%08lx\n", step,
               static_cast<unsigned long>(hr));
  return true;
}

} // namespace

namespace toast {

Action parseAction(const std::wstring& arguments)
{
  if (arguments == L"action=reply")
    return Action::Reply;
  if (arguments == L"action=mark-read")
    return Action::MarkRead;
  return Action::Open;
}

bool show(const std::wstring& aumid, const Content& content,
          std::function<void(Action action, const std::wstring& reply)> activated)
{
  ComPtr<IInspectable> inspectable;
  HRESULT hr = RoActivateInstance(
    HStringReference(RuntimeClass_Windows_Data_Xml_Dom_XmlDocument).Get(),
    &inspectable);
  if (failed(hr, "XmlDocument"))
    return false;
  ComPtr<IXmlDocument> document;
  ComPtr<IXmlDocumentIO> io;
  if (failed(inspectable.As(&document), "IXmlDocument") ||
      failed(document.As(&io), "IXmlDocumentIO"))
    return false;
  const std::wstring text = xml(content);
  if (failed(io->LoadXml(HStringReference(text.c_str()).Get()), "LoadXml"))
    return false;

  ComPtr<IToastNotificationFactory> factory;
  ComPtr<IToastNotification> notification;
  hr = RoGetActivationFactory(
    HStringReference(RuntimeClass_Windows_UI_Notifications_ToastNotification)
      .Get(),
    IID_PPV_ARGS(&factory));
  if (failed(hr, "ToastNotification factory") ||
      failed(factory->CreateToastNotification(document.Get(), &notification),
             "CreateToastNotification"))
    return false;

  ComPtr<IToastNotification2> tagged;
  if (SUCCEEDED(notification.As(&tagged))) {
    tagged->put_Group(HStringReference(L"messages").Get());
    if (!content.tag.empty()) {
      const std::wstring tag = content.tag.substr(0, 64);
      tagged->put_Tag(HStringReference(tag.c_str()).Get());
    }
  }

  EventRegistrationToken token{};
  hr = notification->add_Activated(
    Callback<ActivatedHandler>(
      [activated](IToastNotification*, IInspectable* args) -> HRESULT {
        if (!activated)
          return S_OK;
        const Action action = parseAction(activationArguments(args));
        activated(action,
                  action == Action::Reply ? replyText(args) : std::wstring());
        return S_OK;
      })
      .Get(),
    &token);
  failed(hr, "add_Activated");

  ComPtr<IToastNotificationManagerStatics> manager;
  ComPtr<IToastNotifier> notifier;
  hr = RoGetActivationFactory(
    HStringReference(
      RuntimeClass_Windows_UI_Notifications_ToastNotificationManager)
      .Get(),
    IID_PPV_ARGS(&manager));
  if (failed(hr, "ToastNotificationManager") ||
      failed(manager->CreateToastNotifierWithId(
               HStringReference(aumid.c_str()).Get(), &notifier),
             "CreateToastNotifierWithId") ||
      failed(notifier->Show(notification.Get()), "Show"))
    return false;

  std::lock_guard guard(g_lock);
  g_kept.push_back(notification);
  while (g_kept.size() > kKept)
    g_kept.pop_front();
  return true;
}

void shutdown()
{
  std::lock_guard guard(g_lock);
  g_kept.clear();
}

} // namespace toast
