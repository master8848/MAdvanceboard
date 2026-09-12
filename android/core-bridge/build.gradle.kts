plugins {
    alias(libs.plugins.android.library)
}

android {
    namespace = "com.kb.bridge"
    compileSdk = 36

    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

dependencies {
    implementation(libs.coroutines.core)
    // JNA backs the UniFFI-generated `uniffi.kbcore` bindings (vendored
    // output of `uniffi-bindgen 0.32.1`, matching `uniffi 0.32.1` in
    // core-rust/Cargo.toml). The `@aar` packaging is REQUIRED (not the
    // plain jar): it carries jni/<abi>/libjnidispatch.so, without which
    // JNA fails on-device with "libjnidispatch.so not found in resource
    // path" (caught live on the Pixel_4 smoke). Version stays single-
    // sourced in the catalog (`libs.versions.jna`).
    implementation("net.java.dev.jna:jna:${libs.versions.jna.get()}@aar")
    testImplementation(libs.junit4)
}
