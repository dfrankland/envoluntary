use std::{fs, process::Output};

use assert_cmd::Command;

pub mod common;
use common::{TestVals, build_test_vals};

const NUSHELL_ENV_CHECK: &str = r#"
let json_data = '{{.JsonBlob}}' | from json

let invalid = ($json_data | items {|k, v| if ($env | get -o $k) != $v { $"($k): ($env | get -o $k) instead of ($v)" } } | compact)

if ($invalid | length) > 0 {
    error make { msg: $"Invalid environment variables: ($invalid | str join ', ')" }
} else {
    print "All variables correct in environment."
}
"#;

fn load_nu_env_string(test_vals: &TestVals, current_dir: &str) -> String {
    format!(
            "envoluntary shell export nushell --config-path {config_path} --cache-dir {cache_dir} --current-dir {current_dir} | from json --objects | default {{}} | reduce --fold {{}} {{|row, acc| $acc | merge $row}}
 | load-env",
            config_path = &test_vals.config_file.to_string_lossy(),
            cache_dir = &test_vals.cache_dir.path().to_string_lossy(),
            current_dir = current_dir
        )
}

#[derive(Default)]
struct Inputs<'a> {
    current_dir: String,
    expected_env: serde_json::Value,
    test_vals: Option<&'a TestVals>,
    home_dir: Option<String>,
    extra_script: Option<String>,
}

impl<'a> Inputs<'a> {
    fn new(current_dir: &str, expected_env: serde_json::Value) -> Self {
        Inputs {
            current_dir: current_dir.to_string(),
            expected_env,
            ..Default::default()
        }
    }
}

fn export_and_check(inputs: &Inputs) -> Output {
    let Inputs {
        expected_env,
        current_dir,
        test_vals: maybe_test_vals,
        home_dir,
        extra_script,
    } = inputs;
    let default_test_vals = build_test_vals();
    let test_vals = maybe_test_vals.unwrap_or(&default_test_vals);
    let mut cmd = Command::new("nu");
    let env_check_script = NUSHELL_ENV_CHECK.replace("{{.JsonBlob}}", &expected_env.to_string());
    let script = if let Some(ext_script) = extra_script {
        ext_script.clone().to_owned() + ";\n" + &env_check_script
    } else {
        env_check_script
    };
    cmd.args([
        "-c",
        &format!(
            "{load_env};\n{script}",
            load_env = load_nu_env_string(test_vals, current_dir),
            script = script
        ),
    ])
    .env("PATH", &test_vals.path)
    .env(
        "HOME",
        home_dir.clone().unwrap_or_else(|| "/home".to_string()),
    );

    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn test_nushell_export_state_init_update_and_reset() {
    let test_vals = build_test_vals();

    // --- TEST CASES ---

    // Case 1: No matching path
    export_and_check(&Inputs::new(
        "/no-match-path",
        serde_json::json!({
            "FAKE_VAR": null,
            "ENVOLUNTARY_ENV_STATE": null,
        }),
    ));

    // Case 2: Initial export
    let expected_json = serde_json::json!({
        "FAKE_VAR": "true",
        "ENVOLUNTARY_ENV_STATE": "KLUv/QQ4dQMArAYAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9yZXBvIl0sImVudl92YXJzX3Jlc2V0Ijp7IkZBS0VfVkFSIjpudWxsLCJFTlZPTFVOVEFSWV9FTlZfU1RBVEUiOm51bGx9fQCbvTM7"
    });
    export_and_check(&Inputs::new("/some/dir", expected_json));

    // // Case 3: Update state
    let expected_update_json = serde_json::json!({
        "FAKE_VAR": "true",
        "ENVOLUNTARY_ENV_STATE": "KLUv/QQ4HQQAHAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1YjpvdGhlcl9fb3duZXIvcmVwbyJdLCJlbnZfdmFyc19yZXNldCI6eyJGQUtFX1ZBUiI6bnVsbCwiRU5WT0xVTlRBUllfRU5WX1NUQVRFIjpudWxsfX0BqBDj//0Qww8Qow+DMUZWbCwq",
    });

    export_and_check(&Inputs {
        extra_script: Some(load_nu_env_string(&test_vals, "/home/some/other/dir")),
        ..Inputs::new("/some/dir", expected_update_json)
    });

    // Case 4: Export in a dir, re-export in same dir
    export_and_check(&Inputs {
        extra_script: Some(load_nu_env_string(&test_vals, "/home/some/other/dir")),
        ..Inputs::new("/home/some/other/dir", serde_json::json!({}))
    });

    // Case 5: Reset state
    export_and_check(&Inputs {
        extra_script: Some(load_nu_env_string(&test_vals, "/")),
        ..Inputs::new(
            "/home/some/other/dir",
            serde_json::json!({"ENVOLUNTARY_ENV_STATE": null}),
        )
    });

    // Case 6: Adjacent pattern matching
    let adjacent_test_vals = build_test_vals();
    let bin_dir = &adjacent_test_vals.bin_dir.to_string_lossy().into_owned();
    export_and_check(&Inputs {
        test_vals: Some(&adjacent_test_vals),
        ..Inputs::new(bin_dir, serde_json::json!({"ENVOLUNTARY_ENV_STATE": null}))
    });

    fs::File::create_new(adjacent_test_vals.work_dir.path().join(".supercooltool")).unwrap();
    export_and_check(&Inputs {
        test_vals: Some(&adjacent_test_vals),
        ..Inputs::new(
            bin_dir,
            serde_json::json!({
                "FAKE_VAR": "true",
                "ENVOLUNTARY_ENV_STATE": "KLUv/QQ4zQMAXAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9zdXBlcl9jb29sX3Rvb2wiXSwiZW52X3ZhcnNfcmVzZXQiOnsiRkFLRV9WQVIiOm51bGwsIkVOVk9MVU5UQVJZX0VOVl9TVEFURSI6bnVsbH19ACCjTjk="
            }),
        )
    });

    // Case 7: Home directory adjacent pattern
    let home_dir = tempfile::tempdir().unwrap();
    let home_path = home_dir.path().to_string_lossy();
    fs::File::create_new(home_dir.path().join(".awesometool")).unwrap();

    export_and_check(&Inputs {
        home_dir: Some(home_path.to_string()),
        ..Inputs::new(
            &home_path,
            serde_json::json!({
                "FAKE_VAR": "true",
                "ENVOLUNTARY_ENV_STATE": "KLUv/QQ4tQMALAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9hd2Vzb21lX3Rvb2wiXSwiZW52X3ZhcnNfcmVzZXQiOnsiRkFLRV9WQVIiOm51bGwsIkVOVk9MVU5UQVJZX0VOVl9TVEFURSI6bnVsbH19AB6hxkc="
            }),
        )
    });
}
