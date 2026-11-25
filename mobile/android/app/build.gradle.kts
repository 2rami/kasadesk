plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "com.kasa.kasadesk.mobile"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        applicationId = "com.kasa.kasadesk.mobile"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        if (System.getenv("KASADESK_ANDROID_KEYSTORE") != null) {
            create("distribution") {
                storeFile = file(System.getenv("KASADESK_ANDROID_KEYSTORE"))
                storePassword = System.getenv("KASADESK_ANDROID_PASSWORD")
                keyAlias = "kasadesk"
                keyPassword = storePassword
            }
        }
    }
    buildTypes {
        release { signingConfig = signingConfigs.findByName("distribution") }
    }
}

flutter {
    source = "../.."
}
