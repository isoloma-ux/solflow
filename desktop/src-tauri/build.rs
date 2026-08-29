fn main() {
    link_sherpa();
    tauri_build::build()
}

/// Линковка sherpa-onnx для разделения говорящих. Без фичи `diarize`
/// пропускается целиком.
///
/// На macOS библиотеки собраны свои (см. память проекта) и линкуются
/// статикой, чтобы бандл остался одним файлом. На Windows берётся готовая
/// сборка k2-fsa: там DLL и импортные библиотеки — статические собраны под
/// другую разновидность рантайма и с Rust не сходятся, а DLL мы и так возим
/// (модули движка).
fn link_sherpa() {
    if std::env::var("CARGO_FEATURE_DIARIZE").is_err() {
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "windows" {
        let sherpa = root.join("sherpa/win/lib");
        println!("cargo:rustc-link-search=native={}", sherpa.display());
        println!("cargo:rustc-link-lib=dylib=sherpa-onnx-c-api");
        println!("cargo:rustc-link-lib=dylib=onnxruntime");
        println!("cargo:rerun-if-changed=sherpa/win/lib");
        return;
    }

    if target_os != "macos" {
        panic!(
            "фича diarize собрана под macOS и Windows: под {target_os} нужны свои \
             библиотеки sherpa-onnx — собирайте с --no-default-features"
        );
    }

    let sherpa = root.join("sherpa/lib");
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
