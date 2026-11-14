#include "heaptrace.h"

#ifdef _WIN32
#include <windows.h>

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace {

using HeapAllocFn = LPVOID(WINAPI*)(HANDLE, DWORD, SIZE_T);
using HeapReAllocFn = LPVOID(WINAPI*)(HANDLE, DWORD, LPVOID, SIZE_T);
using HeapFreeFn = BOOL(WINAPI*)(HANDLE, DWORD, LPVOID);
using VirtualAllocFn = LPVOID(WINAPI*)(LPVOID, SIZE_T, DWORD, DWORD);
using VirtualFreeFn = BOOL(WINAPI*)(LPVOID, SIZE_T, DWORD);

HeapAllocFn realAlloc = nullptr;
HeapReAllocFn realReAlloc = nullptr;
HeapFreeFn realFree = nullptr;
VirtualAllocFn realVAlloc = nullptr;
VirtualFreeFn realVFree = nullptr;

// The tracer's own tables live in a heap of their own, through the real
// functions, so they never show up in (or recurse into) what they count.
HANDLE privateHeap = nullptr;
SRWLOCK lock = SRWLOCK_INIT;
bool installed = false;
char prefix[MAX_PATH] = {};

constexpr int kDepth = 28;

struct Stack
{
  uint64_t hash;
  uint32_t depth;
  uint32_t kind; // 0 heap, 1 virtual
  int64_t bytes;
  int64_t count;
  void* frames[kDepth];
};

struct Block
{
  void* ptr; // nullptr empty, (void*)1 tombstone
  size_t size;
  uint32_t stack;
};

Stack* stacks = nullptr;
uint32_t stackCount = 0, stackCap = 0;
uint32_t* stackIndex = nullptr; // open addressing over stack ids + 1
uint32_t stackIndexCap = 0;

Block* blocks = nullptr;
size_t blockCap = 0, blockUsed = 0, blockLive = 0;

struct Region
{
  uintptr_t start, end;
  uint32_t stack;
};
Region* regions = nullptr;
size_t regionCount = 0, regionCap = 0;

int64_t heapLive = 0, virtLive = 0;

void* raw(size_t n) { return realAlloc(privateHeap, HEAP_ZERO_MEMORY, n); }

uint64_t mix(uint64_t h) { h ^= h >> 33; h *= 0xff51afd7ed558ccdULL; h ^= h >> 33; return h; }

uint32_t internStack(void** frames, uint32_t depth, uint32_t kind)
{
  uint64_t h = 1469598103934665603ULL ^ kind;
  for (uint32_t i = 0; i < depth; ++i)
    h = (h ^ reinterpret_cast<uintptr_t>(frames[i])) * 1099511628211ULL;
  if (stackIndexCap == 0 || (stackCount + 1) * 2 > stackIndexCap) {
    const uint32_t cap = stackIndexCap ? stackIndexCap * 2 : 1u << 16;
    auto* index = static_cast<uint32_t*>(raw(sizeof(uint32_t) * cap));
    for (uint32_t i = 0; i < stackCount; ++i) {
      uint32_t at = uint32_t(mix(stacks[i].hash)) & (cap - 1);
      while (index[at]) at = (at + 1) & (cap - 1);
      index[at] = i + 1;
    }
    if (stackIndex) realFree(privateHeap, 0, stackIndex);
    stackIndex = index;
    stackIndexCap = cap;
  }
  uint32_t at = uint32_t(mix(h)) & (stackIndexCap - 1);
  while (const uint32_t id = stackIndex[at]) {
    const Stack& s = stacks[id - 1];
    if (s.hash == h && s.depth == depth && s.kind == kind &&
        std::memcmp(s.frames, frames, depth * sizeof(void*)) == 0)
      return id - 1;
    at = (at + 1) & (stackIndexCap - 1);
  }
  if (stackCount == stackCap) {
    const uint32_t cap = stackCap ? stackCap * 2 : 1u << 15;
    auto* grown = static_cast<Stack*>(raw(sizeof(Stack) * cap));
    if (stacks) {
      std::memcpy(grown, stacks, sizeof(Stack) * stackCount);
      realFree(privateHeap, 0, stacks);
    }
    stacks = grown;
    stackCap = cap;
  }
  Stack& s = stacks[stackCount];
  s.hash = h;
  s.depth = depth;
  s.kind = kind;
  std::memcpy(s.frames, frames, depth * sizeof(void*));
  stackIndex[at] = stackCount + 1;
  return stackCount++;
}

size_t slotOf(void* p, size_t cap) { return size_t(mix(reinterpret_cast<uintptr_t>(p) >> 4)) & (cap - 1); }

void growBlocks()
{
  const size_t cap = blockCap ? blockCap * 2 : size_t(1) << 20;
  auto* grown = static_cast<Block*>(raw(sizeof(Block) * cap));
  for (size_t i = 0; i < blockCap; ++i) {
    void* p = blocks[i].ptr;
    if (!p || p == reinterpret_cast<void*>(1)) continue;
    size_t at = slotOf(p, cap);
    while (grown[at].ptr) at = (at + 1) & (cap - 1);
    grown[at] = blocks[i];
  }
  if (blocks) realFree(privateHeap, 0, blocks);
  blocks = grown;
  blockCap = cap;
  blockUsed = blockLive;
}

void addBlock(void* p, size_t size, uint32_t stack)
{
  if ((blockUsed + 1) * 10 > blockCap * 7) growBlocks();
  size_t at = slotOf(p, blockCap);
  size_t tomb = SIZE_MAX;
  while (blocks[at].ptr) {
    if (blocks[at].ptr == p) { // reused without a free we saw
      Stack& old = stacks[blocks[at].stack];
      old.bytes -= int64_t(blocks[at].size);
      old.count -= 1;
      heapLive -= int64_t(blocks[at].size);
      --blockLive;
      tomb = at;
      break;
    }
    if (blocks[at].ptr == reinterpret_cast<void*>(1) && tomb == SIZE_MAX) tomb = at;
    at = (at + 1) & (blockCap - 1);
  }
  if (tomb != SIZE_MAX) at = tomb; else ++blockUsed;
  blocks[at] = { p, size, stack };
  ++blockLive;
  stacks[stack].bytes += int64_t(size);
  stacks[stack].count += 1;
  heapLive += int64_t(size);
}

void removeBlock(void* p)
{
  if (!p || !blockCap) return;
  size_t at = slotOf(p, blockCap);
  while (blocks[at].ptr) {
    if (blocks[at].ptr == p) {
      Stack& s = stacks[blocks[at].stack];
      s.bytes -= int64_t(blocks[at].size);
      s.count -= 1;
      heapLive -= int64_t(blocks[at].size);
      blocks[at].ptr = reinterpret_cast<void*>(1);
      --blockLive;
      return;
    }
    at = (at + 1) & (blockCap - 1);
  }
}

__declspec(noinline) uint32_t capture(uint32_t kind)
{
  void* frames[kDepth];
  const USHORT n = RtlCaptureStackBackTrace(2, kDepth, frames, nullptr);
  return internStack(frames, n, kind);
}

LPVOID WINAPI hookAlloc(HANDLE heap, DWORD flags, SIZE_T size)
{
  LPVOID p = realAlloc(heap, flags, size);
  if (p) {
    AcquireSRWLockExclusive(&lock);
    addBlock(p, size, capture(0));
    ReleaseSRWLockExclusive(&lock);
  }
  return p;
}

LPVOID WINAPI hookReAlloc(HANDLE heap, DWORD flags, LPVOID old, SIZE_T size)
{
  LPVOID p = realReAlloc(heap, flags, old, size);
  if (p) {
    AcquireSRWLockExclusive(&lock);
    removeBlock(old);
    addBlock(p, size, capture(0));
    ReleaseSRWLockExclusive(&lock);
  }
  return p;
}

BOOL WINAPI hookFree(HANDLE heap, DWORD flags, LPVOID p)
{
  if (p) {
    AcquireSRWLockExclusive(&lock);
    removeBlock(p);
    ReleaseSRWLockExclusive(&lock);
  }
  return realFree(heap, flags, p);
}

// Committed ranges, kept as a list (there are few of them); a partial
// decommit splits a range, so the list is rebuilt into a second buffer.
Region* scratch = nullptr;

void ensureRegions(size_t need)
{
  if (need <= regionCap) return;
  size_t cap = regionCap ? regionCap : 4096;
  while (cap < need) cap *= 2;
  auto* grown = static_cast<Region*>(raw(sizeof(Region) * cap));
  if (regions) { std::memcpy(grown, regions, sizeof(Region) * regionCount); realFree(privateHeap, 0, regions); }
  if (scratch) realFree(privateHeap, 0, scratch);
  regions = grown;
  scratch = static_cast<Region*>(raw(sizeof(Region) * cap));
  regionCap = cap;
}

void dropRange(uintptr_t start, uintptr_t end)
{
  ensureRegions(regionCount * 2 + 2);
  size_t out = 0;
  for (size_t i = 0; i < regionCount; ++i) {
    const Region r = regions[i];
    if (r.end <= start || r.start >= end) { scratch[out++] = r; continue; }
    const uintptr_t lo = r.start > start ? r.start : start, hi = r.end < end ? r.end : end;
    stacks[r.stack].bytes -= int64_t(hi - lo);
    virtLive -= int64_t(hi - lo);
    if (r.start < start) scratch[out++] = { r.start, start, r.stack };
    if (r.end > end) scratch[out++] = { end, r.end, r.stack };
    if (r.start >= start && r.end <= end) stacks[r.stack].count -= 1;
  }
  Region* t = regions; regions = scratch; scratch = t;
  regionCount = out;
}

void addRange(uintptr_t start, uintptr_t end, uint32_t stack)
{
  dropRange(start, end);
  ensureRegions(regionCount + 1);
  regions[regionCount++] = { start, end, stack };
  stacks[stack].bytes += int64_t(end - start);
  stacks[stack].count += 1;
  virtLive += int64_t(end - start);
}

LPVOID WINAPI hookVAlloc(LPVOID address, SIZE_T size, DWORD type, DWORD protect)
{
  LPVOID p = realVAlloc(address, size, type, protect);
  if (p && (type & MEM_COMMIT)) {
    const uintptr_t start = reinterpret_cast<uintptr_t>(p) & ~uintptr_t(0xfff);
    const uintptr_t end = (reinterpret_cast<uintptr_t>(p) + size + 0xfff) & ~uintptr_t(0xfff);
    AcquireSRWLockExclusive(&lock);
    addRange(start, end, capture(1));
    ReleaseSRWLockExclusive(&lock);
  }
  return p;
}

BOOL WINAPI hookVFree(LPVOID address, SIZE_T size, DWORD type)
{
  uintptr_t start = reinterpret_cast<uintptr_t>(address) & ~uintptr_t(0xfff), end = start;
  if (type & MEM_RELEASE) {
    // The whole reservation goes.
    MEMORY_BASIC_INFORMATION m;
    uintptr_t at = start;
    while (VirtualQuery(reinterpret_cast<void*>(at), &m, sizeof m) &&
           m.AllocationBase == address) {
      at = reinterpret_cast<uintptr_t>(m.BaseAddress) + m.RegionSize;
    }
    end = at;
  } else if (type & MEM_DECOMMIT) {
    end = (reinterpret_cast<uintptr_t>(address) + size + 0xfff) & ~uintptr_t(0xfff);
  }
  const BOOL ok = realVFree(address, size, type);
  if (ok && end > start) {
    AcquireSRWLockExclusive(&lock);
    dropRange(start, end);
    ReleaseSRWLockExclusive(&lock);
  }
  return ok;
}

// Points every import slot of this exe that resolves to `target` at `hook`.
int patch(void* target, void* hook)
{
  if (!target) return 0;
  auto* base = reinterpret_cast<uint8_t*>(GetModuleHandleW(nullptr));
  auto* dos = reinterpret_cast<IMAGE_DOS_HEADER*>(base);
  auto* nt = reinterpret_cast<IMAGE_NT_HEADERS*>(base + dos->e_lfanew);
  const IMAGE_DATA_DIRECTORY& dir = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
  if (!dir.VirtualAddress) return 0;
  int patched = 0;
  for (auto* d = reinterpret_cast<IMAGE_IMPORT_DESCRIPTOR*>(base + dir.VirtualAddress); d->Name; ++d) {
    for (auto* t = reinterpret_cast<IMAGE_THUNK_DATA*>(base + d->FirstThunk); t->u1.Function; ++t) {
      if (reinterpret_cast<void*>(t->u1.Function) != target) continue;
      DWORD old;
      VirtualProtect(&t->u1.Function, sizeof(void*), PAGE_READWRITE, &old);
      t->u1.Function = reinterpret_cast<ULONGLONG>(hook);
      VirtualProtect(&t->u1.Function, sizeof(void*), old, &old);
      ++patched;
    }
  }
  return patched;
}

// The whole process, for what the hooks cannot see: private commit by
// allocation, each tagged as a heap's (segments and large blocks, from
// HeapWalk), a thread stack (a guard page), write-combined (the GPU
// driver's upload memory), traced VirtualAlloc, or the rest, whose largest
// allocations are listed.
void breakdown(FILE* f, int64_t tracedVirtual, size_t tracer)
{
  static uintptr_t heapBases[32768];
  int heapBaseCount = 0;
  auto addBase = [&](const void* p) {
    MEMORY_BASIC_INFORMATION m;
    if (!VirtualQuery(p, &m, sizeof m)) return;
    const uintptr_t b = reinterpret_cast<uintptr_t>(m.AllocationBase);
    for (int i = 0; i < heapBaseCount; ++i) if (heapBases[i] == b) return;
    if (heapBaseCount < 32768) heapBases[heapBaseCount++] = b;
  };
  HANDLE heaps[256];
  const DWORD count = GetProcessHeaps(256, heaps);
  unsigned long long heapCommitted = 0;
  for (DWORD i = 0; i < count && i < 256; ++i) {
    if (heaps[i] == privateHeap) continue;
    HEAP_SUMMARY summary = {};
    summary.cb = sizeof summary;
    if (HeapSummary(heaps[i], 0, &summary)) {
      heapCommitted += summary.cbCommitted;
      if (summary.cbCommitted >= 512 * 1024)
        std::fprintf(f, "# heap %p%s committed=%zu allocated=%zu\n", heaps[i],
                     heaps[i] == GetProcessHeap() ? " (process)" : "", summary.cbCommitted, summary.cbAllocated);
    }
    if (!HeapLock(heaps[i])) continue;
    PROCESS_HEAP_ENTRY e = {};
    while (HeapWalk(heaps[i], &e)) {
      if (e.wFlags & PROCESS_HEAP_REGION) addBase(e.lpData);
      else if ((e.wFlags & PROCESS_HEAP_ENTRY_BUSY) && e.cbData >= 128 * 1024) addBase(e.lpData);
    }
    HeapUnlock(heaps[i]);
  }
  struct Alloc { uintptr_t base; unsigned long long bytes; DWORD protect; int regions; };
  static Alloc rest[8192];
  int restCount = 0;
  unsigned long long priv = 0, inHeaps = 0, stack = 0, wc = 0, traced = 0, other = 0;
  MEMORY_BASIC_INFORMATION m;
  uintptr_t at = 0;
  uintptr_t base = 0, allocEnd = 0;
  unsigned long long bytes = 0;
  bool guard = false, writeCombined = false;
  DWORD protect = 0;
  int regionsHere = 0;
  auto flush = [&] {
    if (!bytes) return;
    bool heap = false;
    for (int i = 0; i < heapBaseCount && !heap; ++i) heap = heapBases[i] == base;
    bool tracedHere = false;
    for (size_t i = 0; i < regionCount && !tracedHere; ++i)
      tracedHere = regions[i].start >= base && regions[i].start < allocEnd;
    if (heap) inHeaps += bytes;
    else if (guard) stack += bytes;
    else if (writeCombined) wc += bytes;
    else if (tracedHere) traced += bytes;
    else {
      other += bytes;
      if (restCount < 8192) rest[restCount++] = { base, bytes, protect, regionsHere };
    }
  };
  while (VirtualQuery(reinterpret_cast<void*>(at), &m, sizeof m)) {
    const uintptr_t b = reinterpret_cast<uintptr_t>(m.AllocationBase);
    if (b != base) { flush(); base = b; bytes = 0; guard = writeCombined = false; protect = 0; regionsHere = 0; }
    if (m.Type == MEM_PRIVATE && m.State == MEM_COMMIT) {
      priv += m.RegionSize;
      bytes += m.RegionSize;
      ++regionsHere;
      if (!protect) protect = m.Protect;
      if (m.Protect & PAGE_GUARD) guard = true;
      if (m.Protect & PAGE_WRITECOMBINE) writeCombined = true;
    }
    const uintptr_t next = reinterpret_cast<uintptr_t>(m.BaseAddress) + m.RegionSize;
    allocEnd = next;
    if (next <= at) break;
    at = next;
  }
  flush();
  std::fprintf(f, "# private committed=%llu heaps=%llu (HeapSummary %llu) stacks=%llu writecombine=%llu traced_virtual=%llu (live %lld) rest=%llu tracer=%zu\n",
               priv, inHeaps, heapCommitted, stack, wc, traced, static_cast<long long>(tracedVirtual), other, tracer);
  std::fprintf(stderr, "[heap] process: private %.1f MB = heaps %.1f + stacks %.1f + write-combined %.1f + traced virtual %.1f + rest %.1f (tracer %.1f inside)\n",
               priv / 1048576.0, inHeaps / 1048576.0, stack / 1048576.0, wc / 1048576.0, traced / 1048576.0,
               other / 1048576.0, tracer / 1048576.0);
  // The largest of the rest.
  for (int k = 0; k < 25 && k < restCount; ++k) {
    int best = k;
    for (int i = k + 1; i < restCount; ++i) if (rest[i].bytes > rest[best].bytes) best = i;
    const Alloc t = rest[k]; rest[k] = rest[best]; rest[best] = t;
    std::fprintf(f, "# rest %llx %.2f MB prot=%lx regions=%d\n", static_cast<unsigned long long>(rest[k].base),
                 rest[k].bytes / 1048576.0, rest[k].protect, rest[k].regions);
  }
}

} // namespace

namespace heaptrace {

bool active() { return installed; }

void install()
{
  const DWORD n = GetEnvironmentVariableA("YAP_DEBUG_HEAP", prefix, sizeof prefix);
  if (n == 0 || n >= sizeof prefix) return;
  HMODULE k32 = GetModuleHandleW(L"kernel32.dll");
  HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
  HMODULE kbase = GetModuleHandleW(L"kernelbase.dll");
  realAlloc = reinterpret_cast<HeapAllocFn>(GetProcAddress(ntdll, "RtlAllocateHeap"));
  realReAlloc = reinterpret_cast<HeapReAllocFn>(GetProcAddress(ntdll, "RtlReAllocateHeap"));
  realFree = reinterpret_cast<HeapFreeFn>(GetProcAddress(ntdll, "RtlFreeHeap"));
  realVAlloc = reinterpret_cast<VirtualAllocFn>(GetProcAddress(kbase, "VirtualAlloc"));
  realVFree = reinterpret_cast<VirtualFreeFn>(GetProcAddress(kbase, "VirtualFree"));
  // YAP_DEBUG_HEAP_HOOK=0: no hooks, only the process breakdown (the
  // numbers without the tracer's own cost or slowdown).
  char hook[8] = {};
  if (GetEnvironmentVariableA("YAP_DEBUG_HEAP_HOOK", hook, sizeof hook) && hook[0] == '0') {
    installed = true;
    std::fprintf(stderr, "[heap] breakdown only (no hooks) into %s.*.txt\n", prefix);
    return;
  }
  privateHeap = HeapCreate(0, 0, 0);
  AcquireSRWLockExclusive(&lock);
  growBlocks();
  ReleaseSRWLockExclusive(&lock);
  int count = 0;
  const HMODULE modules[] = { k32, ntdll, kbase };
  for (HMODULE m : modules) {
    if (!m) continue;
    count += patch(GetProcAddress(m, m == ntdll ? "RtlAllocateHeap" : "HeapAlloc"), reinterpret_cast<void*>(&hookAlloc));
    count += patch(GetProcAddress(m, m == ntdll ? "RtlReAllocateHeap" : "HeapReAlloc"), reinterpret_cast<void*>(&hookReAlloc));
    count += patch(GetProcAddress(m, m == ntdll ? "RtlFreeHeap" : "HeapFree"), reinterpret_cast<void*>(&hookFree));
    if (m != ntdll) {
      count += patch(GetProcAddress(m, "VirtualAlloc"), reinterpret_cast<void*>(&hookVAlloc));
      count += patch(GetProcAddress(m, "VirtualFree"), reinterpret_cast<void*>(&hookVFree));
    }
  }
  installed = count > 0;
  std::fprintf(stderr, "[heap] tracing %d import slots into %s.*.txt\n", count, prefix);
}

void dump(const char* tag)
{
  if (!installed) return;
  if (!privateHeap) {
    // Breakdown only (YAP_DEBUG_HEAP_HOOK=0).
    char path[MAX_PATH + 64];
    std::snprintf(path, sizeof path, "%s.%s.txt", prefix, tag);
    if (FILE* f = std::fopen(path, "wb")) {
      std::fprintf(f, "# heaptrace %s breakdown only\n", tag);
      breakdown(f, 0, 0);
      std::fclose(f);
    }
    return;
  }
  // Copy what is live under the lock (through the private heap), then
  // write without it: the file writes allocate through the hooks.
  AcquireSRWLockExclusive(&lock);
  const uint32_t n = stackCount;
  auto* copy = static_cast<Stack*>(raw(sizeof(Stack) * (n ? n : 1)));
  uint32_t live = 0;
  for (uint32_t i = 0; i < n; ++i)
    if (stacks[i].bytes != 0 || stacks[i].count != 0) copy[live++] = stacks[i];
  const int64_t heapBytes = heapLive, virtBytes = virtLive;
  const size_t liveBlocks = blockLive;
  const size_t overhead = blockCap * sizeof(Block) + stackCap * sizeof(Stack) +
                          stackIndexCap * sizeof(uint32_t) + regionCap * 2 * sizeof(Region);
  ReleaseSRWLockExclusive(&lock);

  char path[MAX_PATH + 64];
  std::snprintf(path, sizeof path, "%s.%s.txt", prefix, tag);
  FILE* f = std::fopen(path, "wb");
  if (!f) { realFree(privateHeap, 0, copy); return; }
  const uintptr_t exe = reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
  std::fprintf(f, "# heaptrace %s heap_live=%lld heap_blocks=%zu virt_live=%lld stacks=%u overhead=%zu\n",
               tag, static_cast<long long>(heapBytes), liveBlocks, static_cast<long long>(virtBytes), live, overhead);
  breakdown(f, virtBytes, overhead);
  // Module names, cached by base.
  struct Mod { uintptr_t base; char name[64]; };
  static Mod mods[256];
  static int modCount = 0;
  for (uint32_t i = 0; i < live; ++i) {
    const Stack& s = copy[i];
    std::fprintf(f, "%c %lld %lld", s.kind ? 'V' : 'H', static_cast<long long>(s.bytes), static_cast<long long>(s.count));
    for (uint32_t k = 0; k < s.depth; ++k) {
      const uintptr_t a = reinterpret_cast<uintptr_t>(s.frames[k]);
      HMODULE m = nullptr;
      GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                         reinterpret_cast<LPCWSTR>(a), &m);
      const uintptr_t mb = reinterpret_cast<uintptr_t>(m);
      if (mb == exe) { std::fprintf(f, " r%llx", static_cast<unsigned long long>(a - exe)); continue; }
      if (!m) { std::fprintf(f, " ?%llx", static_cast<unsigned long long>(a)); continue; }
      int j = 0;
      while (j < modCount && mods[j].base != mb) ++j;
      if (j == modCount && modCount < 256) {
        wchar_t wide[MAX_PATH];
        GetModuleFileNameW(m, wide, MAX_PATH);
        const wchar_t* slash = wcsrchr(wide, L'\\');
        WideCharToMultiByte(CP_UTF8, 0, slash ? slash + 1 : wide, -1, mods[j].name, sizeof mods[j].name, nullptr, nullptr);
        mods[j].base = mb;
        ++modCount;
      }
      std::fprintf(f, " %s+%llx", j < modCount ? mods[j].name : "?", static_cast<unsigned long long>(a - mb));
    }
    std::fputc('\n', f);
  }
  std::fclose(f);
  realFree(privateHeap, 0, copy);
  std::fprintf(stderr, "[heap] dump %s: heap %.1f MB in %zu blocks, virtual %.1f MB, tracer %.1f MB\n", tag,
               heapBytes / 1048576.0, liveBlocks, virtBytes / 1048576.0, overhead / 1048576.0);
}

} // namespace heaptrace

#else
namespace heaptrace {
void install() {}
bool active() { return false; }
void dump(const char*) {}
}
#endif
