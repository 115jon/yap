#include "toast_win.h"

// The WinRT ABI through WRL, not C++/WinRT: the static Qt was built with
// another C++/WinRT version, and the linker refuses to mix two (their
// inline code would collide).

#include <cstdio>
#include <deque>
#include <mutex>

#include <roapi.h>
#include <windows.data.xml.dom.h>
#include <windows.ui.notifications.h>
#include <wrl/client.h>
#include <wrl/event.h>
#include <wrl/wrappers/corewrappers.h>

using ABI::Windows::Data::Xml::Dom::IXmlDocument;
using ABI::Windows::Data::Xml::Dom::IXmlDocumentIO;
using ABI::Windows::Foundation::ITypedEventHandler;
using ABI::Windows::UI::Notifications::IToastNotification;
using ABI::Windows::UI::Notifications::IToastNotification2;
using ABI::Windows::UI::Notifications::IToastNotificationFactory;
using ABI::Windows::UI::Notifications::IToastNotificationManagerStatics;
using ABI::Windows::UI::Notifications::IToastNotifier;
using ABI::Windows::UI::Notifications::ToastNotification;
using Microsoft::WRL::Callback;
using Microsoft::WRL::ComPtr;
using Microsoft::WRL::Wrappers::HStringReference;

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

std::wstring xml(const toast::Content& content)
{
  std::wstring out = L"<toast><visual><binding template=\"ToastGeneric\">";
  out += L"<text hint-maxLines=\"1\">" + escape(content.title) + L"</text>";
  if (!content.summary.empty())
    out += L"<text hint-maxLines=\"1\">" + escape(content.summary) + L"</text>";
  out += L"<text>" + escape(content.body) + L"</text>";
  out += L"</binding></visual><audio silent=\"true\"/></toast>";
  return out;
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

bool show(const std::wstring& aumid, const Content& content,
          std::function<void()> activated)
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
      [activated](IToastNotification*, IInspectable*) -> HRESULT {
        if (activated)
          activated();
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
