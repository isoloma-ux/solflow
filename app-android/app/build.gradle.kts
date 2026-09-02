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
        // Версия общая для всех систем: релиз один на троих, и проверка
        // обновлений сравнивает номер с тегом. Раньше телефон жил своей
        // нумерацией (2.2) и потому считал 0.2.x старее себя.
        versionCode = 33
        versionName = "0.7.0"

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

        // Ключи Яндекс OAuth — из одного файла на все три платформы в корне
        // репозитория (десктоп читает его же). Пустые — синхронизация в
        // настройках объясняет, что ключи не заданы.
        val yandex = groovy.json.JsonSlurper()
            .parse(rootProject.file("../yandex-oauth.json")) as Map<*, *>
        buildConfigField("String", "YANDEX_CLIENT_ID", "\"${yandex["client_id"] ?: ""}\"")
        buildConfigField("String", "YANDEX_CLIENT_SECRET", "\"${yandex["client_secret"] ?: ""}\"")
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.31.6"
        }
    }

    // Ключ подписи берётся из переменных окружения — так его не нужно
    // держать в репозитории. Без них собирается отладочная подпись, и
    // сборка на чужой машине не ломается.
    val keystorePath = System.getenv("ANDROID_KEYSTORE_FILE")
    if (keystorePath != null && file(keystorePath).exists()) {
        signingConfigs.create("release") {
            storeFile = file(keystorePath)
            storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("ANDROID_KEY_ALIAS") ?: "solflow"
            keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
                ?: System.getenv("ANDROID_KEYSTORE_PASSWORD")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Свой ключ, если он есть: только подписанные им сборки ставятся
            // поверх друг друга. Отладочный годится для проверок на своём
            // телефоне, но живёт год — на нём нельзя строить обновления.
            signingConfig = signingConfigs.findByName("release")
                ?: signingConfigs.getByName("debug")
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
    // Потянуть список встреч вниз — синхронизация с Диском.
    implementation("androidx.swiperefreshlayout:swiperefreshlayout:1.1.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    // Синхронизация фоном: с задержкой после правок и раз в час.
    implementation("androidx.work:work-runtime-ktx:2.10.0")
}
