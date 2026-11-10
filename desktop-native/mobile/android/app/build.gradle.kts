// The prototype's Android shell: Compose views over the Rust core
// (../rust, built by cargo-ndk into src/main/jniLibs, its Kotlin bindings
// generated into src/main/java/uniffi). See ../README.md.
plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.yap.coreproto"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.yap.coreproto"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures { compose = true }
}

kotlin { jvmToolchain(17) }

dependencies {
    val bom = platform("androidx.compose:compose-bom:2026.06.01")
    implementation(bom)
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.9.0")
    implementation("androidx.core:core-ktx:1.16.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.2")
    // UniFFI's Kotlin bindings call the library through JNA.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
}
