plugins {
    id("com.android.application")
}

android {
    namespace = "com.handy.voice"
    compileSdk = 36
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "com.handy.voice"
        minSdk = 26
        targetSdk = 36
        versionCode = 22
        versionName = "2.2"

        ndk {
            // Только arm64: 32-битных телефонов, которым нужна была бы armeabi-v7a,
            // на практике уже не осталось, а каждая лишняя ABI удваивает размер APK.
            abiFilters += "arm64-v8a"
        }

        externalNativeBuild {
            cmake {
                arguments += listOf(
                    "-DANDROID_STL=c++_shared",
                    "-DGGML_CPU_ARM_ARCH=armv8.2-a+dotprod+fp16",
                )
            }
        }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.31.6"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Подписываем отладочным ключом: APK ставится сайдлоадом, в Play он
            // не поедет, а отдельный keystore пока только мешал бы.
            signingConfig = signingConfigs.getByName("debug")
        }
        debug {
            // Нативный код в debug компилируется с -O0, и ggml от этого
            // замедляется примерно в 25 раз (0.4x realtime против 11x).
            // Для отладки Kotlin этого достаточно, но замерять скорость
            // можно только на release-сборке.
            isJniDebuggable = true
        }
    }

    buildFeatures {
        viewBinding = true
        // Версия и код сборки уходят в отчёт о проблеме на экране «О проекте».
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

}

kotlin {
    compilerOptions {
        // JNI sherpa-onnx ищет у колбэка прогресса специализированный
        // invoke(IIJ)Integer — такой метод даёт только старый классовый
        // кодоген лямбд. Новый invokedynamic после desugaring оставляет
        // лишь generic-вариант, и диаризация падала с NoSuchMethodError.
        freeCompilerArgs.add("-Xlambdas=class")
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.constraintlayout:constraintlayout:2.2.0")
    implementation("androidx.recyclerview:recyclerview:1.3.2")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
}
