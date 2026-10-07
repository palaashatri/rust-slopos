from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable, cast

import pytest


CHECKER_PATH = Path(__file__).with_name("check-release-runner-inventory.py")
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
    runner_records = [
        {
            "id": "first-gate",
            "label": "First gate",
            "script": "scripts/run-first-qa.sh",
        },
        {
            "id": "second-gate",
            "label": "Second gate",
            "script": "scripts/run-second-qa.sh",
        },
    ]
    for record in runner_records:
        script = root / record["script"]
        script.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
        script.chmod(0o755)
    manifest = {"schema_version": 1, "runners": runner_records}
    (root / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )
    write_runner(root, manifest)
    return manifest


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
    first_gate = 'run_gate "First gate" bash scripts/run-first-qa.sh\n'
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


@pytest.mark.parametrize(
    ("failed_gate", "exit_status", "completed", "attempted"),
    [
        ("Rust formatting", 17, 1, 2),
        ("First gate", 23, 4, 5),
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
    (tmp_path / "scripts" / "run-first-qa.sh").write_text(
        'printf "first\\n" >> "$TRACE_FILE"\n'
        f"exit {exit_status}\n",
        encoding="utf-8",
    )
    (tmp_path / "scripts" / "run-second-qa.sh").write_text(
        'printf "second\\n" >> "$TRACE_FILE"\n', encoding="utf-8"
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
    if failed_gate == "First gate":
        expected_trace += [
            "cargo clippy --workspace --all-targets --locked -- -D warnings",
            "cargo test --workspace --locked",
            "first",
        ]
    assert trace_path.read_text(encoding="utf-8").splitlines() == expected_trace


def test_rejects_synchronized_aggregate_runner_registration(tmp_path: Path) -> None:
    manifest = write_fixture(tmp_path)
    manifest["runners"][0]["script"] = "scripts/run-release-qa.sh"
    (tmp_path / "qa" / "release-runner-inventory.json").write_text(
        json.dumps(manifest), encoding="utf-8"
    )
    write_runner(tmp_path, manifest)

    errors = validate_inventory(tmp_path)

    assert errors == [
        "release runner inventory must not register the aggregate runner"
    ]


@pytest.mark.parametrize(
    "payload",
    [
        "hash -p /usr/bin/true bash",
        "PATH=/tmp/fake-bin:$PATH",
        "BASH_ENV=/tmp/skip-required-gates.sh",
        "cd /tmp",
        "exit 0",
        'skip=exit\n"$skip" 0',
        'if [[ -x scripts/run-first-qa.sh ]]; then\n'
        '  run_gate "First gate" bash scripts/run-first-qa.sh\n'
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
    (tmp_path / "scripts" / "run-first-qa.sh").unlink()

    errors = validate_inventory(tmp_path)

    assert any("required release runner is missing" in error for error in errors)


def test_rejects_non_executable_required_runner(tmp_path: Path) -> None:
    write_fixture(tmp_path)
    (tmp_path / "scripts" / "run-first-qa.sh").chmod(0o644)

    errors = validate_inventory(tmp_path)

    assert any("required release runner is not executable" in error for error in errors)


def test_rejects_symlinked_required_runner(tmp_path: Path) -> None:
    write_fixture(tmp_path)
    first_runner = tmp_path / "scripts" / "run-first-qa.sh"
    first_runner.unlink()
    first_runner.symlink_to("run-second-qa.sh")

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
