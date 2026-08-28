fn main() {
    link_sherpa();
    tauri_build::build()
}

/// sherpa-onnx с onnxruntime внутри — статикой, чтобы бандл остался одним
/// файлом. Библиотеки собраны build-macos (см. память проекта), порядок
/// важен: c-api → core → зависимости. Под Windows таких библиотек ещё нет,
/// поэтому там диаризация собирается только вместе с ними: без фичи
/// `diarize` линковка пропускается целиком.
fn link_sherpa() {
    if std::env::var("CARGO_FEATURE_DIARIZE").is_err() {
        return;
    }
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        panic!(
            "фича diarize собрана только под macOS: под {target_os} нужны свои \
             статические библиотеки sherpa-onnx — собирайте с --no-default-features"
        );
    }

    let sherpa = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sherpa/lib");
    println!("cargo:rustc-link-search=native={}", sherpa.display());
    for lib in [
        "sherpa-onnx-c-api",
        "sherpa-onnx-core",
        "sherpa-onnx-fstfar",
        "sherpa-onnx-fst",
        "sherpa-onnx-kaldifst-core",
        "kaldi-decoder-core",
        "kaldi-native-fbank-core",
        "kissfft-float",
        "ssentencepiece_core",
        "onnxruntime",
        "onnxruntime_mlas_arm64",
    ] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreML");
    println!("cargo:rerun-if-changed=sherpa/lib");
}
