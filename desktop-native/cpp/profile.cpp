#include "profile.h"

#include <iterator>

#include <windows.h>
#include <shellapi.h>

namespace profile {

namespace {

constexpr size_t kMaxLength = 32;
constexpr wchar_t kSwitch[] = L"--profile";

std::wstring envProfile()
{
  wchar_t buffer[kMaxLength + 2];
  const DWORD n = GetEnvironmentVariableW(L"YAP_PROFILE", buffer,
                                          static_cast<DWORD>(std::size(buffer)));
  // Missing, or too long to be valid (n counts the terminator then).
  if (n == 0 || n >= std::size(buffer))
    return {};
  return std::wstring(buffer, n);
}

// The switch's value if the command line has one: `found` says whether the
// switch was there at all (a bad or missing value still means "default").
std::wstring switchProfile(bool& found)
{
  found = false;
  int argc = 0;
  LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
  if (!argv)
    return {};
  std::wstring value;
  const std::wstring key = kSwitch;
  for (int i = 1; i < argc && !found; ++i) {
    const std::wstring arg = argv[i];
    if (arg == key) {
      found = true;
      if (i + 1 < argc)
        value = argv[i + 1];
    } else if (arg.size() > key.size() && arg.compare(0, key.size(), key) == 0 &&
               arg[key.size()] == L'=') {
      found = true;
      value = arg.substr(key.size() + 1);
    }
  }
  LocalFree(argv);
  return value;
}

std::wstring resolve()
{
  bool found = false;
  std::wstring chosen = switchProfile(found);
  if (!found)
    chosen = envProfile();
  return isValid(chosen) ? chosen : std::wstring();
}

} // namespace

bool isValid(const std::wstring& name)
{
  if (name.empty() || name.size() > kMaxLength)
    return false;
  for (const wchar_t c : name) {
    const bool ok = (c >= L'a' && c <= L'z') || (c >= L'0' && c <= L'9') ||
                    c == L'-';
    if (!ok)
      return false;
  }
  return true;
}

const std::wstring& name()
{
  static const std::wstring value = resolve();
  return value;
}

std::wstring suffix(wchar_t separator)
{
  return name().empty() ? std::wstring() : std::wstring(1, separator) + name();
}

std::wstring dataRoot()
{
  wchar_t base[MAX_PATH];
  const DWORD n = GetEnvironmentVariableW(L"LOCALAPPDATA", base, MAX_PATH);
  if (n == 0 || n >= MAX_PATH)
    return {};
  std::wstring root = std::wstring(base, n) + L"\\YapNative";
  if (!name().empty())
    root += L"\\profiles\\" + name();
  return root;
}

std::wstring displayName()
{
  return name().empty() ? std::wstring(L"Yap Native")
                        : L"Yap Native (" + name() + L")";
}

} // namespace profile
