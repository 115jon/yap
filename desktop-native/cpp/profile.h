#pragma once

// Named profiles: YAP_PROFILE=<name> or `--profile <name>` runs a second,
// fully separate copy of the app (own folders, instance lock, AUMID, tray,
// Run value and credential), so two accounts can run side by side. The name
// is [a-z0-9-]{1,32}; anything else is ignored and the default profile runs.
// The switch wins over the variable. The Rust half (yap-cache's profile
// module) applies the same rules to the same inputs.
//
// No Qt in here: it runs before the log file starts and before
// QGuiApplication exists.

#include <string>

namespace profile {

// Whether `name` is a usable profile name.
bool isValid(const std::wstring& name);

// The profile of this process, or an empty string for the default one.
// Read once from the command line and the environment.
const std::wstring& name();

// "" for the default profile, else `separator` + the name.
std::wstring suffix(wchar_t separator);

// %LOCALAPPDATA%\YapNative, or ...\YapNative\profiles\<name> for a named
// profile. Empty without LOCALAPPDATA. Native (backslash) separators.
std::wstring dataRoot();

// The name to show people: "Yap Native", or "Yap Native (<name>)".
std::wstring displayName();

} // namespace profile
