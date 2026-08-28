plugins {
    // AGP 9 включает поддержку Kotlin сама — отдельный kotlin.android плагин
    // не нужен и приводит к ошибке применения.
    id("com.android.application") version "9.3.2" apply false
}
