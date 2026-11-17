wit_bindgen::generate!({
    path: "../../../../../docs/abi/edict-target-provider.wit",
    world: "lowerer",
});

use edict::target_provider::protocol::{
    Artifact, Diagnostic, DiagnosticSeverity, LoweringOutputArtifact, LoweringSuccessV1,
    ProviderRefusalKind, ProviderRefusalV1,
};

const REVIEWED_TARGET_IR: &[u8] =
    include_bytes!("../../../../../target-ir/canonical/echo-effectful.target-ir.cbor");

struct Fixture;

impl Guest for Fixture {
    fn lower(request: LoweringRequestV1) -> LoweringResultV1 {
        let mode = request
            .requested_outputs
            .first()
            .map(|output| output.role.as_str())
            .unwrap_or_default();
        match mode {
            "fixture.loop" => loop {
                core::hint::spin_loop();
            },
            "fixture.fuel-burn" => {
                burn_fuel();
                valid(request)
            }
            "fixture.trap" => panic!("explicit provider fixture trap"),
            "fixture.memory" => {
                let mut pressure = Vec::new();
                pressure.resize(128 * 1024 * 1024, 0_u8);
                drop(pressure);
                valid(request)
            }
            "fixture.output-flood" => output_flood(request),
            "fixture.diagnostic-flood" => diagnostic_flood(),
            "fixture.schema-invalid" => schema_invalid(request),
            "fixture.noncanonical" => noncanonical(request),
            "fixture.wrong-domain" => wrong_domain(request),
            "fixture.duplicate-role" => duplicate_role(request),
            "fixture.undeclared-output" => undeclared_output(request),
            "fixture.path-traversal" => path_traversal(request),
            "fixture.target-ir" => target_ir(request),
            "fixture.bad-envelope" => Ok(LoweringSuccessV1 {
                outputs: Vec::new(),
                diagnostics: Vec::new(),
            }),
            "fixture.refusal" => Err(ProviderRefusalV1 {
                kind: ProviderRefusalKind::UnsupportedSemantics,
                subject: None,
                diagnostics: Vec::new(),
            }),
            _ => valid(request),
        }
    }
}

/// Loop iterations for `fixture.fuel-burn`. The host tests calibrate the
/// resulting fuel need to lie above the former 50,000,000 ceiling and far
/// below the input-scaled ceiling.
const FUEL_BURN_STEPS: u64 = 20_000_000;

fn burn_fuel() {
    let mut accumulator = 0_u64;
    for step in 0..FUEL_BURN_STEPS {
        accumulator = core::hint::black_box(accumulator.wrapping_add(step));
    }
    core::hint::black_box(accumulator);
}

fn schema_invalid(request: LoweringRequestV1) -> LoweringResultV1 {
    let mut result = valid(request);
    if let Ok(success) = &mut result {
        for output in &mut success.outputs {
            output.artifact.bytes = vec![0xf5];
        }
    }
    result
}

fn noncanonical(request: LoweringRequestV1) -> LoweringResultV1 {
    let mut result = valid(request);
    if let Ok(success) = &mut result {
        for output in &mut success.outputs {
            output.artifact.bytes = vec![0x18, 0x00];
        }
    }
    result
}

fn wrong_domain(request: LoweringRequestV1) -> LoweringResultV1 {
    let mut result = valid(request);
    if let Ok(success) = &mut result {
        for output in &mut success.outputs {
            output.artifact.domain = "runtime.wrong-output/v1".to_owned();
        }
    }
    result
}

fn duplicate_role(request: LoweringRequestV1) -> LoweringResultV1 {
    let mut result = valid(request);
    if let Ok(success) = &mut result {
        if let Some(duplicate) = success.outputs.first().cloned() {
            success.outputs.push(duplicate);
        }
    }
    result
}

fn undeclared_output(request: LoweringRequestV1) -> LoweringResultV1 {
    let mut result = valid(request);
    if let Ok(success) = &mut result {
        if let Some(mut undeclared) = success.outputs.first().cloned() {
            undeclared.role = "zz.undeclared".to_owned();
            success.outputs.push(undeclared);
        }
    }
    result
}

fn path_traversal(request: LoweringRequestV1) -> LoweringResultV1 {
    let mut result = valid(request);
    if let Ok(success) = &mut result {
        for output in &mut success.outputs {
            output.logical_path = Some("../escape.cbor".to_owned());
        }
    }
    result
}

fn target_ir(request: LoweringRequestV1) -> LoweringResultV1 {
    let mut result = valid(request);
    if let Ok(success) = &mut result {
        for output in &mut success.outputs {
            output.artifact.bytes = REVIEWED_TARGET_IR.to_vec();
        }
    }
    result
}

fn valid(request: LoweringRequestV1) -> LoweringResultV1 {
    Ok(LoweringSuccessV1 {
        outputs: request
            .requested_outputs
            .into_iter()
            .map(|output| LoweringOutputArtifact {
                role: output.role,
                kind: output.kind,
                artifact: Artifact {
                    domain: output.domain,
                    bytes: vec![0xf6],
                },
                logical_path: None,
            })
            .collect(),
        diagnostics: Vec::new(),
    })
}

fn output_flood(request: LoweringRequestV1) -> LoweringResultV1 {
    Ok(LoweringSuccessV1 {
        outputs: request
            .requested_outputs
            .into_iter()
            .map(|output| LoweringOutputArtifact {
                role: output.role,
                kind: output.kind,
                artifact: Artifact {
                    domain: output.domain,
                    bytes: vec![0; 2 * 1024 * 1024],
                },
                logical_path: None,
            })
            .collect(),
        diagnostics: Vec::new(),
    })
}

fn diagnostic_flood() -> LoweringResultV1 {
    Ok(LoweringSuccessV1 {
        outputs: Vec::new(),
        diagnostics: vec![Diagnostic {
            code: "fixture.diagnostic-flood".to_owned(),
            severity: DiagnosticSeverity::Error,
            message: "x".repeat(2 * 1024 * 1024),
            repair: None,
        }],
    })
}

export!(Fixture);
