// The session token's key in the Android Keystore, for the core
// (rust/src/client.rs, keystore): seal, open and reset through
// android/src/dev/yap/qtproto/TokenVault.java, which the push code uses
// with the app killed too. The core calls from its blocking threads, one
// call at a time; QJniEnvironment attaches such a thread to the JVM, and
// Qt's class loader finds the app's classes from it.

// Q_OS_ANDROID comes from Qt's headers.
#include <QtCore/qglobal.h>

#include <cstddef>
#include <cstdint>

#ifdef Q_OS_ANDROID
#include <QtCore/QJniEnvironment>
#include <QtCore/QJniObject>

#include <algorithm>
#include <vector>

namespace {

constexpr char kVault[] = "dev/yap/qtproto/TokenVault";

using Sink = void (*)(void* out, const std::uint8_t* data, std::size_t len);

// What a call came to (client.rs keystore: the same numbers).
constexpr int kOk = 0;
// The bytes can never open: no key, another key, damaged (TokenVault.open
// returned an empty array).
constexpr int kInvalid = 1;
// The Keystore could not be asked (JNI, memory, a Keystore error): maybe
// next time.
constexpr int kUnavailable = 2;

// TokenVault.<method>(byte[]) -> byte[]: null on a failure, an empty array
// for bytes that can never open (the Java side logs the exception's class
// only).
int transform(const char* method, const std::uint8_t* data, std::size_t len, void* out, Sink sink)
{
  QJniEnvironment env;
  if (!env.isValid() || len > std::size_t(INT32_MAX))
    return kUnavailable;
  jbyteArray input = env->NewByteArray(jsize(len));
  if (!input) {
    env.checkAndClearExceptions();
    return kUnavailable;
  }
  if (len)
    env->SetByteArrayRegion(input, 0, jsize(len), reinterpret_cast<const jbyte*>(data));
  const QJniObject result = QJniObject::callStaticObjectMethod(kVault, method, "([B)[B", input);
  // This thread may live on (a blocking thread of the core's runtime): no
  // local references left behind.
  env->DeleteLocalRef(input);
  if (env.checkAndClearExceptions() || !result.isValid())
    return kUnavailable;
  const auto array = result.object<jbyteArray>();
  const jsize n = env->GetArrayLength(array);
  if (n == 0)
    return kInvalid;
  std::vector<std::uint8_t> bytes(static_cast<std::size_t>(n));
  env->GetByteArrayRegion(array, 0, n, reinterpret_cast<jbyte*>(bytes.data()));
  if (env.checkAndClearExceptions())
    return kUnavailable;
  sink(out, bytes.data(), bytes.size());
  // An opened token: not left in freed memory.
  std::fill(bytes.begin(), bytes.end(), std::uint8_t(0));
  return kOk;
}

} // namespace

// op: 0 seal, 1 open, 2 reset. Returns kOk, kInvalid or kUnavailable.
extern "C" int yap_mobile_keystore(int op, const std::uint8_t* data, std::size_t len, void* out, Sink sink)
{
  switch (op) {
  case 0:
    return transform("seal", data, len, out, sink);
  case 1:
    return transform("open", data, len, out, sink);
  case 2: {
    QJniEnvironment env;
    if (!env.isValid())
      return kUnavailable;
    QJniObject::callStaticMethod<void>(kVault, "reset", "()V");
    return env.checkAndClearExceptions() ? kUnavailable : kOk;
  }
  default:
    return kUnavailable;
  }
}
#endif
