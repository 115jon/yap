#pragma once

// Scrolling with Android's feel, for every Flickable in the window (lists,
// pages, sheets) without each one asking. A list that takes a drag moves
// in step with the finger from the step the drag starts (Qt holds it back
// by how far that step went past the threshold). At a lift the list that had the
// drag flings at the finger's speed as Android's VelocityTracker reads it
// (android/src/dev/yap/qtproto/FlingShim.java), and as far as Android's
// OverScroller would take it from that speed. Qt's own estimate trails a
// flick that speeds up to the lift (one smoothed point per frame, averaged
// over the last three), and its constant friction is right for one speed
// only.
//
// `setprop debug.yap.fling log` logs each touch (the input rate), which
// item has it, how far the list followed the finger, and each fling's
// speed, distance and time (tag yap-fling, and "[fling]" lines); `qt` also
// leaves the flings to Qt, for a comparison. Read at start.

class QQuickWindow;

void installFling(QQuickWindow* window);
