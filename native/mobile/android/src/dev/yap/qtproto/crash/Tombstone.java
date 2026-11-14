package dev.yap.qtproto.crash;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * The few parts of a native crash's tombstone a report needs, read from
 * the protobuf Android 12+ hands over in ApplicationExitInfo's trace
 * (AOSP system/core/debuggerd/proto/tombstone.proto): the signal, the
 * abort message, the probable cause and the crashing thread's backtrace,
 * each frame as its library's file name, offset, function and build id.
 *
 * Everything else in the tombstone is skipped unread: registers, memory
 * around them, the memory map (install paths), open files, the command
 * line and the log buffers (logcat, which can hold anything).
 *
 * A small protobuf reader rather than the protobuf library: five
 * messages, and no dependency. Plain Java, tested on any JVM.
 */
public final class Tombstone {
    /** The most frames kept, and the most tombstone bytes read. */
    static final int MAX_FRAMES = 48;
    static final int MAX_BYTES = 4 * 1024 * 1024;

    public String signal = "";
    public String signalCode = "";
    public boolean hasFaultAddress;
    public long faultAddress;
    public String abortMessage = "";
    public String cause = "";
    public String threadName = "";
    public final List<String> frames = new ArrayList<>();

    private int tid = -1;

    /** The tombstone in `in`, or null when it is not one. */
    public static Tombstone read(InputStream in) throws IOException {
        ByteArrayOutputStream buffer = new ByteArrayOutputStream();
        byte[] chunk = new byte[16 * 1024];
        int n;
        while ((n = in.read(chunk)) > 0) {
            buffer.write(chunk, 0, n);
            if (buffer.size() > MAX_BYTES) return null;
        }
        return parse(buffer.toByteArray());
    }

    /** The tombstone in `bytes`, or null when it is not one. */
    public static Tombstone parse(byte[] bytes) {
        try {
            Tombstone t = new Tombstone();
            // The threads come before or after the tid; keep them raw
            // until the end.
            List<byte[]> threads = new ArrayList<>();
            Reader r = new Reader(bytes, 0, bytes.length);
            while (r.more()) {
                int key = (int) r.varint();
                int field = key >>> 3;
                int wire = key & 7;
                if (field == 6 && wire == 0) {
                    t.tid = (int) r.varint();
                } else if (field == 10 && wire == 2) {
                    t.readSignal(r.message());
                } else if (field == 14 && wire == 2) {
                    t.abortMessage = r.string();
                } else if (field == 15 && wire == 2 && t.cause.isEmpty()) {
                    Reader cause = r.message();
                    while (cause.more()) {
                        int k = (int) cause.varint();
                        if (k >>> 3 == 1 && (k & 7) == 2) t.cause = cause.string();
                        else cause.skip(k & 7);
                    }
                } else if (field == 16 && wire == 2) {
                    threads.add(r.bytes());
                } else {
                    r.skip(wire);
                }
            }
            if (t.tid < 0 && threads.isEmpty() && t.signal.isEmpty()) return null;
            for (byte[] entry : threads) t.readThreadEntry(entry);
            return t;
        } catch (RuntimeException malformed) {
            return null;
        }
    }

    private void readSignal(Reader r) {
        while (r.more()) {
            int key = (int) r.varint();
            int field = key >>> 3;
            int wire = key & 7;
            if (field == 2 && wire == 2) signal = r.string();
            else if (field == 4 && wire == 2) signalCode = r.string();
            else if (field == 8 && wire == 0) hasFaultAddress = r.varint() != 0;
            else if (field == 9 && wire == 0) faultAddress = r.varint();
            else r.skip(wire);
        }
    }

    /** A `threads` map entry: key 1 the tid, value 2 the Thread. */
    private void readThreadEntry(byte[] entry) {
        Reader r = new Reader(entry, 0, entry.length);
        long id = -1;
        byte[] thread = null;
        while (r.more()) {
            int key = (int) r.varint();
            if (key == (1 << 3)) id = r.varint();
            else if (key == ((2 << 3) | 2)) thread = r.bytes();
            else r.skip(key & 7);
        }
        if (thread == null || id != tid) return;
        Reader t = new Reader(thread, 0, thread.length);
        while (t.more()) {
            int key = (int) t.varint();
            int field = key >>> 3;
            int wire = key & 7;
            if (field == 2 && wire == 2) {
                threadName = t.string();
            } else if (field == 4 && wire == 2) {
                Reader frame = t.message();
                if (frames.size() < MAX_FRAMES) frames.add(frame(frames.size(), frame));
            } else {
                t.skip(wire);
            }
        }
    }

    /** `#00 pc 0x1a2b3c libyapqt.so (fn+16) (BuildId: ...)`, as logcat's. */
    private static String frame(int index, Reader r) {
        long relPc = 0;
        long offset = 0;
        String function = "";
        String file = "";
        String buildId = "";
        while (r.more()) {
            int key = (int) r.varint();
            int field = key >>> 3;
            int wire = key & 7;
            if (field == 1 && wire == 0) relPc = r.varint();
            else if (field == 4 && wire == 2) function = r.string();
            else if (field == 5 && wire == 0) offset = r.varint();
            else if (field == 6 && wire == 2) file = r.string();
            else if (field == 8 && wire == 2) buildId = r.string();
            else r.skip(wire);
        }
        StringBuilder out = new StringBuilder(String.format(Locale.ROOT, "#%02d pc 0x%x %s", index,
                relPc, Redactor.fileName(file)));
        if (!function.isEmpty()) out.append(" (").append(function).append('+').append(offset).append(')');
        if (!buildId.isEmpty()) out.append(" (BuildId: ").append(buildId).append(')');
        return out.toString();
    }

    /** Protobuf's wire format: varints, fixed widths, length-delimited. */
    private static final class Reader {
        private final byte[] b;
        private int at;
        private final int end;

        Reader(byte[] b, int at, int end) {
            this.b = b;
            this.at = at;
            this.end = end;
        }

        boolean more() {
            return at < end;
        }

        long varint() {
            long value = 0;
            for (int shift = 0; shift < 64; shift += 7) {
                if (at >= end) throw new IllegalStateException("truncated varint");
                int c = b[at++] & 0xff;
                value |= (long) (c & 0x7f) << shift;
                if ((c & 0x80) == 0) return value;
            }
            throw new IllegalStateException("long varint");
        }

        private int length() {
            long n = varint();
            if (n < 0 || n > end - at) throw new IllegalStateException("bad length");
            return (int) n;
        }

        Reader message() {
            int n = length();
            Reader inner = new Reader(b, at, at + n);
            at += n;
            return inner;
        }

        byte[] bytes() {
            int n = length();
            byte[] out = new byte[n];
            System.arraycopy(b, at, out, 0, n);
            at += n;
            return out;
        }

        String string() {
            int n = length();
            String s = new String(b, at, n, StandardCharsets.UTF_8);
            at += n;
            return s;
        }

        void skip(int wire) {
            switch (wire) {
                case 0 -> varint();
                case 1 -> advance(8);
                case 2 -> advance(length());
                case 5 -> advance(4);
                default -> throw new IllegalStateException("wire type " + wire);
            }
        }

        private void advance(int n) {
            if (n > end - at) throw new IllegalStateException("truncated");
            at += n;
        }
    }
}
