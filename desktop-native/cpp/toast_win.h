#pragma once

// Windows toasts through the WinRT ABI (WRL), apart from Qt: Qt's `signals`
// and `emit` macros collide with the WinRT headers, so this unit includes
// no Qt header. The toast is silent (the app plays its own chime), like the
// desktop app's (tauri-winrt-notification with no sound).

#include <functional>
#include <string>

namespace toast {

struct Content
{
  std::wstring title;
  // The server or channel; omitted when empty.
  std::wstring summary;
  std::wstring body;
  // Replaces an earlier toast with the same tag (at most 64 characters).
  std::wstring tag;
  // A message toast: an inline Reply box (with Send) and Mark as read
  // (desktop-toasts.ts MESSAGE_ACTION_TYPE_ID).
  bool messageActions = false;
};

// What a click on the toast asked for.
enum class Action
{
  Open,     // the toast itself
  Reply,    // Send next to the reply box; `reply` holds the text
  MarkRead, // Mark as read
};

// Shows a toast as `aumid`. `activated` runs on a WinRT thread when the
// toast or one of its buttons is clicked while this process lives (the
// window may be hidden in the tray). Returns false (and logs to stderr)
// when Windows refuses it.
bool show(const std::wstring& aumid, const Content& content,
          std::function<void(Action action, const std::wstring& reply)> activated);

// The action a toast's activation arguments name ("action=reply" and so
// on); anything else opens.
Action parseAction(const std::wstring& arguments);

// Releases the toasts kept for activation. Call before COM goes away
// (aboutToQuit), not from a static destructor.
void shutdown();

} // namespace toast
