fn main() {
    google_credentials();
    link_sherpa();
    link_llama();
    tauri_build::build()
}

/// Ключи Google OAuth: файл лежит в корне репозитория, но в git не
/// попадает (защита GitHub от утечек), поэтому include_str! идёт через
/// OUT_DIR: есть файл — копия, нет — пустые ключи, и вход в Google Drive
/// в окне объяснит, что не настроен.
fn google_credentials() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("../../google-oauth.json");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("google-oauth.json");
    let text = std::fs::read_to_string(&source)
        .unwrap_or_else(|_| "{\"client_id\":\"\",\"client_secret\":\"\"}".to_string());
    std::fs::write(&out, text).unwrap();
    println!("cargo:rerun-if-changed={}", source.display());
}

/// Линковка libsolflow_llama — llama.cpp со своим ggml одной динамической
/// библиотекой (см. llama-shim/). Динамика обязательна: статический ggml
/// движка расшифровки и ggml llama в одном бинаре подрались бы символами.
fn link_llama() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("llama/lib");
    println!("cargo:rustc-check-cfg=cfg(has_summary)");
    if !dir.exists() {
        // Библиотеку собирает llama-shim/build-macos.sh (или CI): без неё
        // собираемся без саммери, чтобы cargo check работал где угодно.
        return;
    }
    println!("cargo:rustc-cfg=has_summary");
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=dylib=solflow_llama");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        // Для запуска из cargo — абсолютный rpath, для бандла — Frameworks.
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }
    println!("cargo:rerun-if-changed=llama/lib");
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

    // Линкуем только то, что есть: свои сборки и готовые от k2-fsa
    // отличаются мелочами (у одних mlas лежит отдельной библиотекой, у
    // других уже внутри onnxruntime). Порядок важен и сохраняется.
    let present = |name: &str| sherpa.join(format!("lib{name}.a")).exists();
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
        if present(lib) {
            println!("cargo:rustc-link-lib=static={lib}");
        }
    }
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreML");
    println!("cargo:rerun-if-changed=sherpa/lib");
}
