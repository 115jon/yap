// YAP_DEBUG_HEAP=<file prefix> (diagnostics, docs/PERFORMANCE.md pass 3):
// live heap and committed VirtualAlloc memory by allocating call stack.
//
// The exe is one static module (Qt, the CRT, Rust, libwebrtc), so every
// allocation it makes goes through its own import slots for HeapAlloc,
// HeapReAlloc, HeapFree, VirtualAlloc and VirtualFree. install() points
// those slots at counting wrappers; allocations made by system DLLs (D3D11,
// the GPU driver, Media Foundation) are not seen and stay the remainder of
// the process's private bytes. Off unless the variable is set.
#pragma once

namespace heaptrace {

// Patches the import slots when YAP_DEBUG_HEAP is set; a no-op otherwise.
void install();
// Whether install() patched anything.
bool active();
// Writes "<prefix>.<tag>.txt": totals, then every live stack by bytes
// (frames as exe RVAs, or module+offset), for tools/p3/heapsym.py.
void dump(const char* tag);

} // namespace heaptrace
