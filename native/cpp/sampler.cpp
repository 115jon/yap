#include "sampler.h"

#ifdef _WIN32
#include <windows.h>

#include <timeapi.h>

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <unordered_map>
#include <vector>

namespace {

constexpr int kDepth = 48;

char g_prefix[MAX_PATH] = {};
HANDLE g_target = nullptr;

struct Sample
{
  int depth;
  DWORD64 frames[kDepth];
};

// Unwinds a suspended thread's context. Touches no heap: the target may
// hold the heap lock while it is suspended.
int unwind(CONTEXT context, DWORD64* frames)
{
  int depth = 0;
  while (depth < kDepth && context.Rip) {
    frames[depth++] = context.Rip;
    DWORD64 imageBase = 0;
    PRUNTIME_FUNCTION function = RtlLookupFunctionEntry(context.Rip, &imageBase, nullptr);
    if (!function) {
      // A leaf without unwind data: its return address is at the stack top.
      // Deeper than the first frame (JIT code, say) the stack is unknown.
      if (depth > 1)
        break;
      context.Rip = *reinterpret_cast<DWORD64*>(context.Rsp);
      context.Rsp += 8;
      continue;
    }
    void* handlerData = nullptr;
    DWORD64 establisher = 0;
    RtlVirtualUnwind(UNW_FLAG_NHANDLER, imageBase, context.Rip, function, &context, &handlerData,
                     &establisher, nullptr);
  }
  return depth;
}

// The nearest export at or below `address` in a system module (ntdll's
// wait stubs, the driver's entry points), so samples in a wait can be told
// apart and DLL time has a name.
const char* nearestExport(HMODULE module, DWORD64 address, DWORD64* exportAddress)
{
  auto* base = reinterpret_cast<const BYTE*>(module);
  auto* dos = reinterpret_cast<const IMAGE_DOS_HEADER*>(base);
  auto* nt = reinterpret_cast<const IMAGE_NT_HEADERS*>(base + dos->e_lfanew);
  const IMAGE_DATA_DIRECTORY& dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT];
  if (!dir.VirtualAddress)
    return nullptr;
  auto* exports = reinterpret_cast<const IMAGE_EXPORT_DIRECTORY*>(base + dir.VirtualAddress);
  auto* functions = reinterpret_cast<const DWORD*>(base + exports->AddressOfFunctions);
  auto* names = reinterpret_cast<const DWORD*>(base + exports->AddressOfNames);
  auto* ordinals = reinterpret_cast<const WORD*>(base + exports->AddressOfNameOrdinals);
  const DWORD64 rva = address - reinterpret_cast<DWORD64>(module);
  const char* best = nullptr;
  DWORD bestRva = 0;
  for (DWORD i = 0; i < exports->NumberOfNames; ++i) {
    const DWORD f = functions[ordinals[i]];
    if (f <= rva && f >= bestRva) {
      bestRva = f;
      best = reinterpret_cast<const char*>(base + names[i]);
    }
  }
  *exportAddress = bestRva;
  return best;
}

std::string moduleOffset(DWORD64 address)
{
  HMODULE module = nullptr;
  char text[MAX_PATH + 32];
  if (GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                         reinterpret_cast<LPCSTR>(address), &module) &&
      module) {
    char path[MAX_PATH] = {};
    GetModuleFileNameA(module, path, MAX_PATH);
    const char* name = std::strrchr(path, '\\');
    name = name ? name + 1 : path;
    const DWORD64 offset = address - reinterpret_cast<DWORD64>(module);
    if (module == GetModuleHandleW(nullptr))
      std::snprintf(text, sizeof text, "exe+%llx", static_cast<unsigned long long>(offset));
    else {
      DWORD64 exportRva = 0;
      if (const char* symbol = nearestExport(module, address, &exportRva))
        std::snprintf(text, sizeof text, "%s!%s+%llx", name, symbol,
                      static_cast<unsigned long long>(offset - exportRva));
      else
        std::snprintf(text, sizeof text, "%s+%llx", name, static_cast<unsigned long long>(offset));
    }
  } else {
    std::snprintf(text, sizeof text, "?+%llx", static_cast<unsigned long long>(address));
  }
  return text;
}

void run(const char* tag, int seconds)
{
  std::vector<Sample> samples;
  samples.reserve(size_t(seconds) * 1100);
  ULONG64 lastCycles = 0;
  QueryThreadCycleTime(g_target, &lastCycles);
  int idle = 0;
  int total = 0;
  const ULONGLONG end = GetTickCount64() + ULONGLONG(seconds) * 1000;
  timeBeginPeriod(1);
  while (GetTickCount64() < end) {
    Sleep(1);
    ++total;
    ULONG64 cycles = 0;
    QueryThreadCycleTime(g_target, &cycles);
    if (cycles == lastCycles) {
      ++idle;
      continue;
    }
    lastCycles = cycles;
    if (SuspendThread(g_target) == DWORD(-1))
      continue;
    CONTEXT context{};
    context.ContextFlags = CONTEXT_FULL;
    if (GetThreadContext(g_target, &context)) {
      Sample sample;
      sample.depth = unwind(context, sample.frames);
      ResumeThread(g_target);
      samples.push_back(sample);
    } else {
      ResumeThread(g_target);
    }
  }
  timeEndPeriod(1);

  // Identical stacks folded, most frequent first.
  std::unordered_map<std::string, int> folded;
  std::unordered_map<DWORD64, std::string> names;
  for (const Sample& sample : samples) {
    std::string key;
    for (int i = 0; i < sample.depth; ++i) {
      auto it = names.find(sample.frames[i]);
      if (it == names.end())
        it = names.emplace(sample.frames[i], moduleOffset(sample.frames[i])).first;
      if (i)
        key += ' ';
      key += it->second;
    }
    ++folded[key];
  }
  std::vector<std::pair<int, const std::string*>> rows;
  for (const auto& [key, count] : folded)
    rows.push_back({ count, &key });
  std::sort(rows.begin(), rows.end(), [](const auto& a, const auto& b) { return a.first > b.first; });

  char path[MAX_PATH + 64];
  std::snprintf(path, sizeof path, "%s.%s.txt", g_prefix, tag);
  if (FILE* out = std::fopen(path, "w")) {
    std::fprintf(out, "# sampler %s: %d ticks, %d running (stacks), %d waiting\n", tag, total,
                 int(samples.size()), idle);
    for (const auto& [count, key] : rows)
      std::fprintf(out, "%d %s\n", count, key->c_str());
    std::fclose(out);
  }
  std::fprintf(stderr, "[sample] %s: %d of %d ticks running, written to %s\n", tag,
               int(samples.size()), total, path);
}

DWORD WINAPI watch(void*)
{
  char trigger[MAX_PATH + 8];
  std::snprintf(trigger, sizeof trigger, "%s.now", g_prefix);
  char value[16] = {};
  const int seconds =
    GetEnvironmentVariableA("YAP_DEBUG_SAMPLE_SECONDS", value, sizeof value) ? (std::max)(1, std::atoi(value)) : 20;
  // Checked at once too, so a trigger written before launch samples the
  // startup.
  for (bool first = true;; first = false) {
    if (!first)
      Sleep(300);
    FILE* in = std::fopen(trigger, "r");
    if (!in)
      continue;
    char tag[64] = {};
    if (!std::fgets(tag, sizeof tag, in))
      std::strcpy(tag, "now");
    std::fclose(in);
    DeleteFileA(trigger);
    for (char* c = tag; *c; ++c)
      if (*c == '\r' || *c == '\n' || *c == ' ')
        *c = 0;
    run(tag[0] ? tag : "now", seconds);
  }
}

} // namespace

namespace sampler {

void install()
{
  const DWORD n = GetEnvironmentVariableA("YAP_DEBUG_SAMPLE", g_prefix, sizeof g_prefix);
  if (n == 0 || n >= sizeof g_prefix)
    return;
  DuplicateHandle(GetCurrentProcess(), GetCurrentThread(), GetCurrentProcess(), &g_target,
                  THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_QUERY_INFORMATION, FALSE, 0);
  if (!g_target)
    return;
  if (HANDLE thread = CreateThread(nullptr, 0, watch, nullptr, 0, nullptr)) {
    SetThreadPriority(thread, THREAD_PRIORITY_TIME_CRITICAL);
    CloseHandle(thread);
  }
  std::fprintf(stderr, "[sample] GUI thread sampling on %s.now\n", g_prefix);
}

} // namespace sampler

#else
namespace sampler {
void install() {}
} // namespace sampler
#endif
