use memlink_sandbox::{
    RestrictedProcessSandbox, Sandbox, SandboxError, SandboxLanguage, SandboxRequest,
};

#[tokio::test]
async fn runs_python_in_temp_process() {
    let sandbox = RestrictedProcessSandbox::default();
    let result = sandbox
        .execute(SandboxRequest {
            code: "print(21 * 2)".to_owned(),
            language: SandboxLanguage::Python,
            input_refs: vec![],
            timeout_ms: 2_000,
            max_output_bytes: 1024,
        })
        .await
        .expect("execute python");

    assert!(result.success);
    assert!(result.stdout.contains("42"));
}

#[tokio::test]
async fn times_out_long_running_shell_process() {
    let sandbox = RestrictedProcessSandbox::default();
    let error = sandbox
        .execute(SandboxRequest {
            code: "sleep 5".to_owned(),
            language: SandboxLanguage::Shell,
            input_refs: vec![],
            timeout_ms: 50,
            max_output_bytes: 1024,
        })
        .await
        .expect_err("timeout expected");

    assert!(matches!(error, SandboxError::Timeout(50)));
}
