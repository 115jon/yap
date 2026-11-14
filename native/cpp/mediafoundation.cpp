#include "mediafoundation.h"

#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <iterator>

#include <windows.h>
#include <delayimp.h>

// From the patched webrtc-sys (patches/webrtc-sys/0006): whether its
// decoders may use Media Foundation.
extern "C" void yap_mf_set_available(bool available);

namespace {

// The DLLs CMakeLists.txt delay-loads. Only Media Foundation code calls
// into them (Qt Multimedia's windows backend, the H.264 decoder and
// encoder, libwebrtc's DMO audio path).
const char* const kDelayed[] = { "mfplat.dll", "mf.dll",    "mfreadwrite.dll",
                                 "evr.dll",    "dxva2.dll", "msdmo.dll" };
// The ones Windows N/KN lacks without the Media Feature Pack.
const wchar_t* const kNeeded[] = { L"mfplat.dll", L"mf.dll", L"mfreadwrite.dll", L"evr.dll" };

std::atomic<bool> g_missing{ false };
bool g_simulated = false;

bool delayed(const char* dll)
{
  if (!dll)
    return false;
  for (const char* name : kDelayed)
    if (_stricmp(dll, name) == 0)
      return true;
  return false;
}

// What every import from a missing DLL becomes. Each of them returns an
// HRESULT (dumpbin /imports: MFStartup, MFCreate*, MFTEnumEx, MFGetService,
// DXVA2CreateDirect3DDeviceManager9, MoInitMediaType, ...). On x64 there is
// one calling convention and the caller cleans the stack, so one stub that
// ignores its arguments fits them all; out parameters are left as they are.
static_assert(sizeof(void*) == 8, "the stub relies on the x64 calling convention");
HRESULT WINAPI missingImport()
{
  return E_NOTIMPL;
}

void logOnce(const char* dll)
{
  static std::atomic<bool> logged{ false };
  if (!logged.exchange(true))
    std::fprintf(stderr, "[media] %s is not used (Media Foundation unavailable); its calls fail with E_NOTIMPL\n", dll);
}

// With Media Foundation unavailable, a delayed DLL is never loaded: the
// helper is handed this exe as its module and the stub for each function.
FARPROC WINAPI notifyHook(unsigned reason, PDelayLoadInfo info)
{
  if (!info || !g_missing.load(std::memory_order_relaxed) || !delayed(info->szDll))
    return nullptr;
  switch (reason) {
    case dliNotePreLoadLibrary:
      logOnce(info->szDll);
      return reinterpret_cast<FARPROC>(GetModuleHandleW(nullptr));
    case dliNotePreGetProcAddress:
      return reinterpret_cast<FARPROC>(&missingImport);
    default:
      return nullptr;
  }
}

// A delayed DLL or function that is not there after all (a call the probe
// did not foresee): the stub, rather than the helper's exception.
FARPROC WINAPI failureHook(unsigned reason, PDelayLoadInfo info)
{
  if (!info || !delayed(info->szDll))
    return nullptr;
  switch (reason) {
    case dliFailLoadLib:
      g_missing.store(true);
      std::fprintf(stderr, "[media] %s failed to load (error %lu)\n", info->szDll,
                   static_cast<unsigned long>(info->dwLastError));
      return reinterpret_cast<FARPROC>(GetModuleHandleW(nullptr));
    case dliFailGetProc:
      return reinterpret_cast<FARPROC>(&missingImport);
    default:
      return nullptr;
  }
}

} // namespace

extern "C" const PfnDliHook __pfnDliNotifyHook2 = notifyHook;
extern "C" const PfnDliHook __pfnDliFailureHook2 = failureHook;

namespace mediafoundation {

void probe()
{
  wchar_t test[8] = {};
  g_simulated = GetEnvironmentVariableW(L"YAP_TEST_NO_MF", test, DWORD(std::size(test))) > 0 &&
                std::wcscmp(test, L"1") == 0;
  const wchar_t* absent = nullptr;
  for (const wchar_t* dll : kNeeded) {
    // Only System32, and only mapped (no code runs), to see that it is there.
    HMODULE module = LoadLibraryExW(dll, nullptr,
                                    LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_AS_DATAFILE);
    if (!module) {
      absent = dll;
      break;
    }
    FreeLibrary(module);
  }
  if (!absent && !g_simulated)
    return;
  g_missing.store(true);
  yap_mf_set_available(false);
  // Screen shares then encode VP8 in libwebrtc (yap-voice mf_h264.rs).
  if (!GetEnvironmentVariableW(L"YAP_SHARE_ENCODER", nullptr, 0))
    SetEnvironmentVariableW(L"YAP_SHARE_ENCODER", L"software");
  if (g_simulated)
    std::fprintf(stderr, "[media] Media Foundation off (YAP_TEST_NO_MF=1): no mp4 playback, "
                         "no H.264 decode, VP8 shares, no cameras\n");
  else
    std::fprintf(stderr, "[media] Media Foundation is missing (%ls; Windows N without the Media "
                         "Feature Pack?): no mp4 playback, no H.264 decode, VP8 shares, no cameras\n",
                 absent);
}

bool available()
{
  return !g_missing.load(std::memory_order_relaxed);
}

bool simulated()
{
  return g_simulated;
}

} // namespace mediafoundation
