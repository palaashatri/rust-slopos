from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable, cast

import pytest


CHECKER_PATH = Path(__file__).with_name("check-release-runner-inventory.py")
EXPECTED_REQUIRED_RUNNER_SCRIPTS = (
    (
        "clean-install",
        "scripts/run-clean-install-qa.sh",
        "a8ae73ba4adc3e55b6eca45505fe4c2be6a90e399fc35473111eca291ccd992b",
    ),
    (
        "catalogue",
        "scripts/run-catalogue-qa.sh",
        "05f726210b411937bd3bee84b32229a1ed454cc221fa5ba1a830aa6710899686",
    ),
    (
        "virtual-services",
        "scripts/run-virtual-services-qa.sh",
        "b2b7a20211db1decabe20c68504bcf30c7a0a9e85fc4a91a44f44aea263d2b9c",
    ),
    (
        "settings-services",
        "scripts/run-settings-service-qa.sh",
        "788f888cd50faeb7cd4a4cb13e144fdfdb0da8b2ae6b4ce9a178b2cce5e3f2c3",
    ),
    (
        "multimonitor",
        "scripts/run-multimonitor-qa.sh",
        "9ef36f254e51d3e5d9d5524b272b71b9788129e835aea3cab0b94e3a5d475599",
    ),
    (
        "resolution",
        "scripts/run-resolution-qa.sh",
        "cf6bfea83b41608586b7447011f6e1004d9e9ba9e62ab995b867b6fc4f4f8ecb",
    ),
    (
        "recovery",
        "scripts/run-recovery-qa.sh",
        "5cb25fa4f5696bc63957e80886d89338515b7a9a119d4d51c30efcfa220af81a",
    ),
    (
        "security-failure",
        "scripts/run-security-failure-qa.sh",
        "9ff6ee540f7f66e528e5646c7a2a20d905dc8204b04c691ec8a254a6d0399b3e",
    ),
    (
        "session-performance",
        "scripts/benchmark-x11-session.sh",
        "d18875f6221e8f06c1eb42df8d9631d44cdbb586e1ad485ee90c90b273944f32",
    ),
    (
        "accessibility-tree",
        "scripts/run-atspi-qa.sh",
        "d2fcb2c5bea48b067356cd65de23789a5b901c290dd3a231e7bc5625bba1868e",
    ),
    (
        "debian-package",
        "scripts/run-debian-package-qa.sh",
        "a2ba5c29ab50ec0a7d44fd091ecd8724d743a8b2eb170942bd374563093b8ae7",
    ),
    (
        "package-repositories",
        "scripts/generate-package-repos.sh",
        "a3287841c29389aa7937d1b454e185d61831f4b09d2493efc579f6d686167f60",
    ),
    (
        "canonical-visual",
        "scripts/run-canonical-visual-qa.sh",
        "d9059acf18dc23cffe2f97d26eef560bff77ab4b4bae9ec235c6dddac52fb0b4",
    ),
)
FIRST_GATE_LABEL = "Gate clean-install"
FIRST_GATE_SCRIPT = "scripts/run-clean-install-qa.sh"
SECOND_GATE_SCRIPT = "scripts/run-catalogue-qa.sh"
CHECKER_SPEC = importlib.util.spec_from_file_location(
    "release_runner_inventory", CHECKER_PATH
)
if CHECKER_SPEC is None or CHECKER_SPEC.loader is None:
    raise ImportError(
        f"Cannot load release runner inventory checker from {CHECKER_PATH}"
    )

CHECKER_MODULE = importlib.util.module_from_spec(CHECKER_SPEC)
CHECKER_SPEC.loader.exec_module(CHECKER_MODULE)
validate_inventory = cast(
    Callable[[Path], list[str]], CHECKER_MODULE.validate_release_runner_inventory
)
render_expected_runner = cast(
    Callable[[list[tuple[str, str]]], str], CHECKER_MODULE.render_expected_runner
)


def write_fixture(root: Path) -> dict[str, Any]:
    (root / "qa").mkdir(parents=True)
    scripts_directory = root / "scripts"
    scripts_directory.mkdir()
    source_root = Path(__file__).resolve().parents[1]
    runner_records = []
    for gate_id, script, expected_sha256 in EXPECTED_REQUIRED_RUNNER_SCRIPTS:
        source_script = source_root / script
        fixture_script = root / script
        content = source_script.read_bytes()
        assert hashlib.sha256(content).hexdigest() == expected_sha256
        fixture_script.write_bytes(content)
        fixture_script.chmod(0o755)
        runner_records.append(
            {
                "id": gate_id,
                "label": f"Gate {gate_id}",
                "script": script,
                "sha256": expected_sha256,
            }
        )
    manifest = {"schema_version": 1, "runners": runner_records}
    write_manifest(root, manifest)
    write_runner(root, manifest)
    return manifest


def write_manifest(root: Path, manifest: dict[str, Any]) -> None:
    (root / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )


def write_runner(root: Path, manifest: dict[str, Any]) -> Path:
    runner_path = root / "scripts" / "run-release-qa.sh"
    calls = [(record["label"], record["script"]) for record in manifest["runners"]]
    runner_path.write_text(render_expected_runner(calls), encoding="utf-8")
    runner_path.chmod(0o755)
    return runner_path


def inject_before_first_manifest_gate(
    root: Path, source: str, suffix: str = ""
) -> Path:
    runner_path = root / "scripts" / "run-release-qa.sh"
    runner_source = runner_path.read_text(encoding="utf-8")
    first_gate = f'run_gate "{FIRST_GATE_LABEL}" bash {FIRST_GATE_SCRIPT}\n'
    assert first_gate in runner_source
    runner_path.write_text(
        runner_source.replace(
            first_gate, source + "\n" + first_gate + suffix + "\n", 1
        ),
        encoding="utf-8",
    )
    return runner_path


def run_checker(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            sys.executable,
            str(CHECKER_PATH),
            "--root",
            str(root),
        ],
        capture_output=True,
        check=False,
        text=True,
    )


def test_repository_inventory_matches_release_runner() -> None:
    root = Path(__file__).resolve().parents[1]

    assert validate_inventory(root) == []


def test_repository_release_runner_has_valid_bash_syntax() -> None:
    runner_path = Path(__file__).resolve().parents[1] / "scripts/run-release-qa.sh"
    syntax = subprocess.run(
        ["bash", "-n", str(runner_path)], capture_output=True, check=False, text=True
    )

    assert syntax.returncode == 0, syntax.stderr


def test_accepts_complete_executable_inventory(tmp_path: Path) -> None:
    write_fixture(tmp_path)

    assert validate_inventory(tmp_path) == []


def test_repository_inventory_matches_canonical_required_runner_scripts() -> None:
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads(
        (root / "qa" / "release-runner-inventory.json").read_text(
            encoding="utf-8"
        )
    )

    assert [
        (entry["id"], entry["script"], entry["sha256"])
        for entry in manifest["runners"]
    ] == list(EXPECTED_REQUIRED_RUNNER_SCRIPTS)
    for _, script, expected_sha256 in EXPECTED_REQUIRED_RUNNER_SCRIPTS:
        assert (
            hashlib.sha256((root / script).read_bytes()).hexdigest()
            == expected_sha256
        )


def test_rejects_omitting_a_required_runner_even_when_template_matches(
    tmp_path: Path,
) -> None:
    manifest = write_fixture(tmp_path)
    manifest["runners"].pop()
    write_manifest(tmp_path, manifest)
    write_runner(tmp_path, manifest)

    errors = validate_inventory(tmp_path)

    assert any("required runner set" in error for error in errors), errors


def test_rejects_rerouting_a_required_runner_to_an_unregistered_script(
    tmp_path: Path,
) -> None:
    manifest = write_fixture(tmp_path)
    replacement_script = "scripts/run-unregistered-gate.sh"
    replacement_path = tmp_path / replacement_script
    replacement_path.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
    replacement_path.chmod(0o755)
    manifest["runners"][0]["script"] = replacement_script
    write_manifest(tmp_path, manifest)
    write_runner(tmp_path, manifest)

    errors = validate_inventory(tmp_path)

    assert any("required runner set" in error for error in errors), errors


def test_rejects_required_runner_body_change_without_digest_update(
    tmp_path: Path,
) -> None:
    manifest = write_fixture(tmp_path)
    script = tmp_path / manifest["runners"][0]["script"]
    script.write_text("#!/usr/bin/env bash\ntrue\n", encoding="utf-8")

    errors = validate_inventory(tmp_path)

    assert any("sha256 mismatch" in error for error in errors), errors


def test_rejects_changed_runner_even_when_manifest_digest_is_updated(
    tmp_path: Path,
) -> None:
    manifest = write_fixture(tmp_path)
    record = manifest["runners"][0]
    script = tmp_path / record["script"]
    changed_content = b"#!/usr/bin/env bash\nexit 0\n"
    script.write_bytes(changed_content)
    record["sha256"] = hashlib.sha256(changed_content).hexdigest()
    write_manifest(tmp_path, manifest)
    write_runner(tmp_path, manifest)

    errors = validate_inventory(tmp_path)

    assert any(
        "required runner set, order, and sha256" in error for error in errors
    ), errors


@pytest.mark.parametrize(
    ("failed_gate", "exit_status", "completed", "attempted"),
    [
        ("Rust formatting", 17, 1, 2),
        (FIRST_GATE_LABEL, 23, 4, 5),
    ],
)
def test_failed_gate_replaces_stale_pass_and_stops_runner(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    failed_gate: str,
    exit_status: int,
    completed: int,
    attempted: int,
) -> None:
    write_fixture(tmp_path)
    (tmp_path / "scripts" / CHECKER_PATH.name).write_bytes(CHECKER_PATH.read_bytes())
    trace_path = tmp_path / "gates.log"
    monkeypatch.setenv("TRACE_FILE", str(trace_path))
    monkeypatch.setenv("SOURCE_SHA", "fixture-source")
    # Keep the clean-install gate's trace to the install step under test.
    monkeypatch.setenv("SLOPOS_QA_SKIP_BUILD", "1")
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    monkeypatch.setenv("PATH", str(fake_bin), prepend=":")
    cargo = fake_bin / "cargo"
    cargo_status = exit_status if failed_gate == "Rust formatting" else 0
    cargo.write_text(
        "#!/usr/bin/env bash\n"
        'printf "cargo %s\\n" "$*" >> "$TRACE_FILE"\n'
        f"exit {cargo_status}\n",
        encoding="utf-8",
    )
    cargo.chmod(0o755)
    # The real clean-install gate invokes install.sh, so stub it to fail here.
    (tmp_path / "install.sh").write_text(
        'printf "first\\n" >> "$TRACE_FILE"\n' f"exit {exit_status}\n",
        encoding="utf-8",
    )
    report_path = tmp_path / "artifacts" / "qa" / "release" / "report.md"
    report_path.parent.mkdir(parents=True)
    report_path.write_text("- Result: **PASS**\n", encoding="utf-8")

    result = subprocess.run(
        ["bash", str(tmp_path / "scripts" / "run-release-qa.sh")],
        capture_output=True,
        check=False,
        text=True,
        timeout=30,
    )

    assert result.returncode == exit_status, result.stdout + result.stderr
    report = report_path.read_text(encoding="utf-8")
    assert "- Result: **FAIL**" in report
    assert "**PASS**" not in report
    assert f"- Failed gate: `{failed_gate}`" in report
    assert f"- Completed gates: {completed} / {attempted} attempted" in report
    assert "- Source commit: `fixture-source`" in report
    assert "RELEASE_QA_EVIDENCE_OK" not in result.stdout
    expected_trace = ["cargo fmt --all -- --check"]
    if failed_gate == FIRST_GATE_LABEL:
        expected_trace += [
            "cargo clippy --workspace --all-targets --locked -- -D warnings",
            "cargo test --workspace --locked",
            "first",
        ]
    assert trace_path.read_text(encoding="utf-8").splitlines() == expected_trace


def test_stale_report_is_removed_when_runner_is_killed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    write_fixture(tmp_path)
    report_path = tmp_path / "artifacts" / "qa" / "release" / "report.md"
    report_path.parent.mkdir(parents=True)
    report_path.write_text("- Result: **PASS**\n", encoding="utf-8")
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    # Kill the aggregate runner during the first gate, before it can write a
    # report. The fake cargo sits under the gate script, so the runner is its
    # grandparent; check the command line before signalling anything.
    killer = fake_bin / "cargo"
    killer.write_text(
        "#!/usr/bin/env bash\n"
        'runner="$(ps -o ppid= -p "$PPID" | tr -d " ")"\n'
        'if tr "\\0" " " < "/proc/$runner/cmdline" | grep -q "run-release-qa.sh"; then\n'
        '  kill -KILL "$runner"\n'
        "fi\n"
        "exit 1\n",
        encoding="utf-8",
    )
    killer.chmod(0o755)
    monkeypatch.setenv("PATH", str(fake_bin), prepend=":")

    result = subprocess.run(
        ["bash", str(tmp_path / "scripts" / "run-release-qa.sh")],
        capture_output=True,
        check=False,
        text=True,
        timeout=30,
    )

    # SIGKILL skips the ERR trap, so only the startup cleanup can remove the
    # previous run's PASS report.
    assert result.returncode == -9, result.stdout + result.stderr
    assert not report_path.exists()


def test_rejects_synchronized_aggregate_runner_registration(tmp_path: Path) -> None:
    manifest = write_fixture(tmp_path)
    manifest["runners"][0]["script"] = "scripts/run-release-qa.sh"
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )
    write_runner(tmp_path, manifest)

    errors = validate_inventory(tmp_path)

    # The path change also breaks the canonical tuple and digest, as it must.
    assert "release runner inventory must not register the aggregate runner" in errors


@pytest.mark.parametrize(
    "payload",
    [
        "hash -p /usr/bin/true bash",
        "PATH=/tmp/fake-bin:$PATH",
        "BASH_ENV=/tmp/skip-required-gates.sh",
        "cd /tmp",
        "exit 0",
        'skip=exit\n"$skip" 0',
        'if [[ -x scripts/run-catalogue-qa.sh ]]; then\n'
        '  run_gate "Gate catalogue" bash scripts/run-catalogue-qa.sh\n'
        "fi",
        "run_gate \"Extra gate\" bash scripts/run-extra-qa.sh",
    ],
)
def test_rejects_noncanonical_commands_before_required_gates(
    tmp_path: Path, payload: str
) -> None:
    write_fixture(tmp_path)
    runner_path = inject_before_first_manifest_gate(tmp_path, payload)
    syntax = subprocess.run(
        ["bash", "-n", str(runner_path)], capture_output=True, check=False, text=True
    )

    errors = validate_inventory(tmp_path)

    assert syntax.returncode == 0, syntax.stderr
    assert any("verified static template and inventory" in error for error in errors)


def test_rejects_comment_split_function_declaration_that_exits_before_gates(
    tmp_path: Path,
) -> None:
    write_fixture(tmp_path)
    runner_path = inject_before_first_manifest_gate(
        tmp_path,
        "skip_gates() # comment\n{\n  exit 0\n}\nskip_gates",
    )
    syntax = subprocess.run(
        ["bash", "-n", str(runner_path)], capture_output=True, check=False, text=True
    )

    errors = validate_inventory(tmp_path)

    assert syntax.returncode == 0, syntax.stderr
    assert any("verified static template and inventory" in error for error in errors)


@pytest.mark.parametrize(
    ("prefix", "suffix"),
    [
        ("if false; then if true; then\n:", "\nfi\nfi"),
        ("! if false; then", "\nfi"),
        ("true; if false; then", "\nfi"),
    ],
)
def test_rejects_valid_nested_or_negated_conditionals(
    tmp_path: Path, prefix: str, suffix: str
) -> None:
    write_fixture(tmp_path)
    runner_path = inject_before_first_manifest_gate(tmp_path, prefix, suffix)
    syntax = subprocess.run(
        ["bash", "-n", str(runner_path)], capture_output=True, check=False, text=True
    )

    errors = validate_inventory(tmp_path)

    assert syntax.returncode == 0, syntax.stderr
    assert any("verified static template and inventory" in error for error in errors)


def test_rejects_runner_with_unregistered_call(tmp_path: Path) -> None:
    write_fixture(tmp_path)
    runner_path = inject_before_first_manifest_gate(
        tmp_path, 'run_gate "Extra gate" bash scripts/run-extra-qa.sh'
    )

    errors = validate_inventory(tmp_path)

    assert any("verified static template and inventory" in error for error in errors)


def test_rejects_runner_calls_outside_manifest_order(tmp_path: Path) -> None:
    manifest = write_fixture(tmp_path)
    manifest["runners"].reverse()
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )

    errors = validate_inventory(tmp_path)

    assert any("verified static template and inventory" in error for error in errors)


def test_rejects_missing_required_runner(tmp_path: Path) -> None:
    write_fixture(tmp_path)
    (tmp_path / FIRST_GATE_SCRIPT).unlink()

    errors = validate_inventory(tmp_path)

    assert any("required release runner is missing" in error for error in errors)


def test_rejects_non_executable_required_runner(tmp_path: Path) -> None:
    write_fixture(tmp_path)
    (tmp_path / FIRST_GATE_SCRIPT).chmod(0o644)

    errors = validate_inventory(tmp_path)

    assert any("required release runner is not executable" in error for error in errors)


def test_rejects_symlinked_required_runner(tmp_path: Path) -> None:
    write_fixture(tmp_path)
    first_runner = tmp_path / FIRST_GATE_SCRIPT
    first_runner.unlink()
    first_runner.symlink_to(Path(SECOND_GATE_SCRIPT).name)

    errors = validate_inventory(tmp_path)

    assert any("symlinked release runner paths" in error for error in errors)


def test_rejects_duplicate_runner_ids_and_paths(tmp_path: Path) -> None:
    manifest = write_fixture(tmp_path)
    manifest["runners"][1]["id"] = manifest["runners"][0]["id"]
    manifest["runners"][1]["script"] = manifest["runners"][0]["script"]
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )

    errors = validate_inventory(tmp_path)

    assert any("repeats id" in error for error in errors), errors
    assert any("repeats script" in error for error in errors), errors


@pytest.mark.parametrize(
    "script",
    [
        "scripts/../outside.sh",
        "scripts/run first-qa.sh",
        "scripts/$(touch-pwned).sh",
        "scripts/[r]un-first-qa.sh",
        "/tmp/outside.sh",
    ],
)
def test_rejects_unsafe_runner_path(tmp_path: Path, script: str) -> None:
    manifest = write_fixture(tmp_path)
    manifest["runners"][0]["script"] = script
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )

    errors = validate_inventory(tmp_path)

    assert any("unsafe script path" in error for error in errors), errors


@pytest.mark.parametrize(
    "label",
    [
        "",
        " Leading space",
        "Shell \"; exit 0; #",
        "$(touch-pwned)",
        "label\nsecond line",
    ],
)
def test_rejects_unsafe_gate_label(tmp_path: Path, label: str) -> None:
    manifest = write_fixture(tmp_path)
    manifest["runners"][0]["label"] = label
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )

    errors = validate_inventory(tmp_path)

    assert any("invalid label" in error for error in errors), errors


def test_rejects_manifest_with_extra_fields(tmp_path: Path) -> None:
    manifest = write_fixture(tmp_path)
    manifest["extra"] = True
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )

    errors = validate_inventory(tmp_path)

    assert any("only schema_version and runners" in error for error in errors)


def test_rejects_unsupported_schema_version(tmp_path: Path) -> None:
    manifest = write_fixture(tmp_path)
    manifest["schema_version"] = 2
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )

    errors = validate_inventory(tmp_path)

    assert any("schema_version must be 1" in error for error in errors)


def test_cli_returns_two_for_malformed_json(tmp_path: Path) -> None:
    write_fixture(tmp_path)
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        "{invalid json", encoding="utf-8"
    )

    result = run_checker(tmp_path)

    assert result.returncode == 2
    assert "Cannot validate release runner inventory" in result.stderr


def test_cli_returns_two_for_duplicate_json_keys(tmp_path: Path) -> None:
    write_fixture(tmp_path)
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        '{"schema_version":1,"schema_version":1,"runners":[]}', encoding="utf-8"
    )

    result = run_checker(tmp_path)

    assert result.returncode == 2
    assert "duplicate JSON object key" in result.stderr


def test_cli_returns_two_for_invalid_utf8_manifest(tmp_path: Path) -> None:
    write_fixture(tmp_path)
    (tmp_path / "qa" / "release-runner-inventory.json").write_bytes(b"\xff")

    result = run_checker(tmp_path)

    assert result.returncode == 2
    assert "Cannot validate release runner inventory" in result.stderr


def test_cli_returns_one_for_invalid_inventory(tmp_path: Path) -> None:
    manifest = write_fixture(tmp_path)
    manifest["runners"] = []
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )

    result = run_checker(tmp_path)

    assert result.returncode == 1
    assert "runners must be a non-empty list" in result.stderr
