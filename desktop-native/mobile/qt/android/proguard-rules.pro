# R8 rules for release builds (build.gradle). Qt's C++ calls its Java side
# over JNI by class, method and field name, and so does Native
# (cpp/native.cpp -> dev.yap.qtproto.NativeShim): none of it may be renamed
# or removed, even when no Java code calls it.
-keep class org.qtproject.qt.android.** { *; }
-keep class dev.yap.** { *; }
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}
# Qt's Java names optional platform classes it never needs on this build.
-dontwarn org.qtproject.qt.android.**
