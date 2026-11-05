#include "logfile.h"

#include <cstdio>
#include <filesystem>
#include <string>
#include <system_error>
#include <thread>

#include <fcntl.h>
#include <io.h>
#include <windows.h>

namespace fs = std::filesystem;

namespace {

constexpr std::uintmax_t kMaxBytes = 5u * 1024 * 1024;
// The live file and yap-native.1.log .. yap-native.4.log.
constexpr int kKeep = 5;

fs::path logDir()
{
  wchar_t base[MAX_PATH];
  const DWORD n = GetEnvironmentVariableW(L"LOCALAPPDATA", base, MAX_PATH);
  if (n == 0 || n >= MAX_PATH)
    return {};
  return fs::path(base) / L"YapNative" / L"logs";
}

fs::path logFile(const fs::path& dir, int index)
{
  return dir / (index == 0 ? std::wstring(L"yap-native.log")
                           : L"yap-native." + std::to_wstring(index) + L".log");
}

void rotate(const fs::path& dir)
{
  std::error_code ec;
  fs::remove(logFile(dir, kKeep - 1), ec);
  for (int i = kKeep - 2; i >= 0; --i)
    fs::rename(logFile(dir, i), logFile(dir, i + 1), ec);
}

HANDLE openLog(const fs::path& dir)
{
  return CreateFileW(logFile(dir, 0).c_str(), FILE_APPEND_DATA,
                     FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                     nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
}

std::string stamp()
{
  SYSTEMTIME t;
  GetLocalTime(&t);
  char text[32];
  std::snprintf(text, sizeof text, "%04u-%02u-%02u %02u:%02u:%02u.%03u ",
                t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond,
                t.wMilliseconds);
  return text;
}

void write(HANDLE handle, const char* data, std::size_t size)
{
  DWORD written = 0;
  if (handle && handle != INVALID_HANDLE_VALUE)
    WriteFile(handle, data, static_cast<DWORD>(size), &written, nullptr);
}

// Reads the stderr pipe until every writer is gone: mirrors the bytes as
// they are, and appends them to the file with each line stamped.
void pump(HANDLE pipe, HANDLE mirror, fs::path dir)
{
  HANDLE file = openLog(dir);
  LARGE_INTEGER size{};
  GetFileSizeEx(file, &size);
  std::uintmax_t written = static_cast<std::uintmax_t>(size.QuadPart);
  bool lineStart = true;
  char buffer[4096];
  std::string out;
  for (;;) {
    DWORD n = 0;
    if (!ReadFile(pipe, buffer, sizeof buffer, &n, nullptr) || n == 0)
      break;
    write(mirror, buffer, n);
    out.clear();
    for (DWORD i = 0; i < n; ++i) {
      if (lineStart) {
        out += stamp();
        lineStart = false;
      }
      out += buffer[i];
      if (buffer[i] == '\n')
        lineStart = true;
    }
    write(file, out.data(), out.size());
    written += out.size();
    if (written > kMaxBytes && lineStart) {
      CloseHandle(file);
      rotate(dir);
      file = openLog(dir);
      written = 0;
    }
  }
  CloseHandle(file);
}

} // namespace

namespace logfile {

void start()
{
  const fs::path dir = logDir();
  if (dir.empty())
    return;
  std::error_code ec;
  fs::create_directories(dir, ec);
  if (fs::file_size(logFile(dir, 0), ec) > kMaxBytes && !ec)
    rotate(dir);

  // The stderr this process was given, if it is a file or a pipe. Not a
  // console: one paused by a selection would block the pump, then every
  // writer in the process.
  HANDLE mirror = nullptr;
  const HANDLE original = GetStdHandle(STD_ERROR_HANDLE);
  if (original && original != INVALID_HANDLE_VALUE &&
      GetFileType(original) != FILE_TYPE_CHAR)
    DuplicateHandle(GetCurrentProcess(), original, GetCurrentProcess(),
                    &mirror, 0, FALSE, DUPLICATE_SAME_ACCESS);

  HANDLE read = nullptr;
  HANDLE writeEnd = nullptr;
  if (!CreatePipe(&read, &writeEnd, nullptr, 0))
    return;
  // Rust's stderr asks for this handle on every write.
  SetStdHandle(STD_ERROR_HANDLE, writeEnd);
  // The C runtime's stderr (fprintf, Qt) onto the same pipe.
  const int fd = _open_osfhandle(reinterpret_cast<intptr_t>(writeEnd), _O_BINARY);
  if (fd >= 0) {
    if (_fileno(stderr) < 0) {
      FILE* unused = nullptr;
      freopen_s(&unused, "NUL", "w", stderr);
    }
    _dup2(fd, _fileno(stderr));
    std::setvbuf(stderr, nullptr, _IONBF, 0);
  }
  std::thread(pump, read, mirror, dir).detach();
}

} // namespace logfile
