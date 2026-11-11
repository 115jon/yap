# R8 rules for release builds (build.gradle). Qt's C++ calls its Java side
# over JNI by class, method and field name, and so does Native
# (cpp/native.cpp -> dev.yap.qtproto.NativeShim, cpp/push.cpp ->
# dev.yap.qtproto.push.PushShim): none of it may be renamed or removed, even
# when no Java code calls it. The push service and receiver are kept by the
# manifest; Firebase brings its own consumer rules.
-keep class org.qtproject.qt.android.** { *; }
-keep class dev.yap.** { *; }
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}
# Qt's Android platform plugin (C++) also calls AndroidX by name: the
# downloadable emoji font (Google Play Services fonts) and file: URLs
# through the manifest's FileProvider.
-keep class androidx.core.provider.FontRequest { *; }
-keep class androidx.core.provider.FontsContractCompat { *; }
-keep class androidx.core.provider.FontsContractCompat$* { *; }
-keepclassmembers class androidx.core.content.FileProvider {
    public static android.net.Uri getUriForFile(...);
}
# Qt's Java names optional platform classes it never needs on this build.
-dontwarn org.qtproject.qt.android.**
