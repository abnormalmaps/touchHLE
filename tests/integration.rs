use std::env;
use std::env::current_dir;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

// adapted from `assert_cmd` crate
fn target_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .map(|mut path| {
            path.pop();
            if path.ends_with("deps") {
                path.pop();
            }
            path
        })
        .unwrap()
}

// https://stackoverflow.com/a/35907071/2241008
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Makes a path object, and checks that it exists.
fn make_path_and_check(
    tests_dir: &Path,
    path: &[&str],
    object: &str,
    is_executable: bool,
) -> PathBuf {
    let mut buf = tests_dir.to_path_buf();
    if is_executable {
        for part in &path[..(path.len() - 1)] {
            buf.push(part);
        }
        buf.push(format!(
            "{}{}",
            path.last().unwrap(),
            env::consts::EXE_SUFFIX
        ));
        println!("{}", buf.iter().last().unwrap().display())
    } else {
        for part in path {
            buf.push(part);
        }
    }

    if !buf.exists() {
        panic!(
            "Couldn't find {} at {}. Please see {} for more details.",
            object,
            buf.display(),
            tests_dir.join("README.md").display()
        );
    }
    buf
}

fn generate_libc_stub<'a, F: std::io::Write, S: AsRef<str>, I: Iterator<Item = S>>(
    output: &mut F,
    symbols: &mut I,
) -> Result<(), Box<dyn Error>> {
    for symbol in symbols {
        // Need to strip off leading underscore
        let symbol = &symbol.as_ref().trim();
        assert!(
            symbol.chars().nth(0).unwrap() == '_',
            "symbol {} does not start with '_'",
            symbol
        );
        writeln!(output, "void {}() {{}}", &symbol[1..])?;
    }
    Ok(())
}

fn build_object<I: Iterator<Item = P>, P: AsRef<OsStr>>(
    tests_dir: &Path,
    output_name: &Path,
    sources: I,
    extra_compile_args: &[&str],
) -> Result<(), Box<dyn Error>> {
    let clang_path = make_path_and_check(tests_dir, &["llvm", "bin", "clang"], "Clang", true);

    let bin_path = make_path_and_check(
        tests_dir,
        &["common-3.0.sdk", "usr", "bin"],
        "binary directory",
        false,
    );

    let sdk_path = make_path_and_check(tests_dir, &["common-3.0.sdk"], "SDK sysroot", false);

    let mut linker_arg = OsString::from("-B");
    linker_arg.push(bin_path);
    let mut sdk_arg = OsString::from("--sysroot=");
    sdk_arg.push(sdk_path);

    eprintln!("Building {} for iPhone OS 3...", output_name.display());
    std::io::stderr().flush().unwrap();
    let mut cmd = Command::new(clang_path);
    let output = cmd
        // Uncomment for verbose output (useful for debugging search path
        // issues)
        // .arg("-v")
        // Uncomment for verbose linker output
        // .arg("-Wl,-v")
        // Target iPhone OS 2
        .arg("--target=arm-apple-ios")
        .arg("-miphoneos-version-min=2.0")
        .args(["-arch", "armv7"])
        // If enabled, the stack protection causes a null pointer crash in some
        // functions. This is probably because ___stack_chk_guard isn't linked.
        .arg("-fno-stack-protector")
        .arg("-DPRODUCT_iPhone")
        .arg(linker_arg)
        .arg(sdk_arg)
        .args(extra_compile_args)
        // Input files.
        .args(sources)
        // Write the output to the bundle.
        .arg("-o")
        .arg(output_name)
        .output()
        .expect("failed to execute Clang process");
    eprintln!("Running {:?}", cmd);
    std::io::stdout().write_all(&output.stdout).unwrap();
    std::io::stderr().write_all(&output.stderr).unwrap();
    assert!(output.status.success());
    eprintln!("Built successfully.");
    Ok(())
}

// Note that source files are looked for in the path
// "{tests_dir}/{test_app_name}_source"
// and binaries are output as
// "{tests_dir}/{test_app_name}.app/{test_app_name}".
fn run_test_app(
    tests_dir: &Path,
    test_app_name: &str,
    sources: &[&Path],
    extra_compile_args: &[&str],
    extra_run_args: &[&str],
) -> Result<(), Box<dyn Error>> {
    let test_app_path = tests_dir.join(format!("{}.app", test_app_name));
    build_object(
        &tests_dir,
        &tests_dir
            .join(format!("{}.app", test_app_name))
            .join(test_app_name),
        sources.iter().map(|file| {
            tests_dir
                .join(format!("{}_source", test_app_name))
                .join(file)
        }),
        extra_compile_args,
    )?;
    let binary_name = "touchHLE";
    let binary_path = target_dir().join(format!("{}{}", binary_name, env::consts::EXE_SUFFIX));
    let mut cmd = Command::new(binary_path);
    let output = cmd
        .arg(test_app_path)
        // headless mode avoids a distracting window briefly appearing during
        // testing, and works in CI.
        .arg("--headless")
        .args(extra_run_args)
        .output()
        .expect("failed to execute touchHLE process");
    std::io::stdout().write_all(&output.stdout).unwrap();
    std::io::stderr().write_all(&output.stderr).unwrap();
    assert!(output.status.success());
    // sanity check: check that emulation actually happened
    assert_ne!(
        find_subsequence(output.stderr.as_slice(), b"CPU emulation begins now."),
        None
    );
    write!(
        &mut std::io::stdout(),
        "Finished running {}.\n\n\n",
        test_app_name
    )
    .unwrap();
    Ok(())
}

#[test]
fn test_app() -> Result<(), Box<dyn Error>> {
    let tests_dir = current_dir()?.join("tests");
    let sdk_libs_dir = "-L".to_owned() + current_dir()?.join("touchHLE_dylibs").to_str().unwrap();
    let libc_stub_dir = "-L".to_owned() + tests_dir.join("libc_stub").to_str().unwrap();
    let extra_compile_args = [
        "-mlinker-version=253",
        "-Wno-expansion-to-defined",
        "-Wno-literal-range",
        sdk_libs_dir.as_str(),
        libc_stub_dir.as_str(),
        "-ObjC",
        "-fno-objc-exceptions",
        // ARC is not available until IOS 5, so it can't be used.
        "-fno-objc-arc",
        "-fno-objc-arc-exceptions",
    ];

    // Generate symbols.
    let symbols_path = tests_dir.join("SYMBOLS.txt");
    let dump_file_option = format!("--dump-file={}", symbols_path.to_str().unwrap());
    let dump_run_args = ["--dump=symbols", dump_file_option.as_str(), "--headless"];
    let binary_name = "touchHLE";
    let binary_path = target_dir().join(format!("{}{}", binary_name, env::consts::EXE_SUFFIX));
    let mut cmd = Command::new(binary_path);
    let output = cmd
        .args(dump_run_args)
        .output()
        .expect("failed to execute touchHLE process");
    assert!(output.status.success());

    let symbols_file = BufReader::new(File::open(symbols_path).unwrap());
    let mut output_file =
        BufWriter::new(File::create(tests_dir.join("libc_stub").join("libc_stub.c")).unwrap());
    generate_libc_stub(
        &mut output_file,
        &mut symbols_file.lines().map(|s| s.unwrap()),
    )
    .unwrap();
    // Close the file so it gets flushed
    std::mem::drop(output_file);
    let libc_compile_args = [
        "-mlinker-version=253",
        "-fno-builtin",
        "-nostdlib",
        &format!(
            "-Wl,-install_name,{}",
            Path::new("usr")
                .join("lib")
                .join("libSystem.B.dylib")
                .to_str()
                .unwrap()
        ),
        "-Wl,-dylib",
    ];
    build_object(
        &tests_dir,
        &tests_dir.join("libc_stub").join("libSystem.dylib"),
        [tests_dir.join("libc_stub").join("libc_stub.c")].iter(),
        &libc_compile_args,
    )
    .unwrap();

    let sources = ["main.m", "SyncTester.m"].map(|file| Path::new(file));
    run_test_app(&tests_dir, "TestApp", &sources, &extra_compile_args, &[])
}
