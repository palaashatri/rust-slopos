from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable, cast

import pytest


CHECKER_PATH = Path(__file__).with_name("check-architecture.py")
if not CHECKER_PATH.is_file():
    raise ModuleNotFoundError("No module named 'check_architecture'")

CHECKER_SPEC = importlib.util.spec_from_file_location(
    "check_architecture", CHECKER_PATH
)
if CHECKER_SPEC is None or CHECKER_SPEC.loader is None:
    raise ImportError(f"Cannot load architecture checker from {CHECKER_PATH}")

CHECKER_MODULE = importlib.util.module_from_spec(CHECKER_SPEC)
CHECKER_SPEC.loader.exec_module(CHECKER_MODULE)
check_architecture = cast(
    Callable[[dict[str, Any], Path], list[str]], CHECKER_MODULE.check_architecture
)

MISSING_RESOLUTION = object()


def run_checker_cli(
    tmp_path: Path, cargo_script: str
) -> subprocess.CompletedProcess[str]:
    fake_bin = tmp_path / "bin"
    fake_bin.mkdir()
    cargo_path = fake_bin / "cargo"
    cargo_path.write_text(f"#!/bin/sh\nset -eu\n{cargo_script}\n", encoding="utf-8")
    cargo_path.chmod(0o755)
    environment = os.environ.copy()
    environment["PATH"] = os.pathsep.join(
        (str(fake_bin), environment.get("PATH", ""))
    )
    return subprocess.run(
        [sys.executable, str(CHECKER_PATH)],
        cwd=CHECKER_PATH.parent.parent,
        env=environment,
        capture_output=True,
        check=False,
        text=True,
    )


def make_dependency(
    package_name: str,
    *,
    rename: str | None = None,
    optional: bool = False,
    kind: str | None = None,
    target: str | None = None,
) -> dict[str, Any]:
    return {
        "name": package_name,
        "rename": rename,
        "optional": optional,
        "kind": kind,
        "target": target,
    }


def make_resolved_dependency(
    package_id: str,
    alias: str,
    *,
    kind: str | None = None,
    target: str | None = None,
) -> dict[str, Any]:
    return {
        "pkg": package_id,
        "name": alias,
        "dep_kinds": [{"kind": kind, "target": target}],
    }


def make_package(
    package_id: str,
    package_name: str,
    manifest_path: Path,
    dependencies: list[dict[str, Any]] | None = None,
    *,
    targets: list[dict[str, Any]] | None = None,
    source: str | None = None,
) -> dict[str, Any]:
    return {
        "id": package_id,
        "name": package_name,
        "manifest_path": str(manifest_path),
        "source": source,
        "dependencies": dependencies or [],
        "targets": targets or [],
    }


def write_manifest(package_root: Path, package_name: str) -> Path:
    package_root.mkdir(parents=True, exist_ok=True)
    manifest_path = package_root / "Cargo.toml"
    manifest_path.write_text(
        f"[package]\nname = '{package_name}'\n", encoding="utf-8"
    )
    return manifest_path


def package_identifier(manifest_path: Path, package_name: str) -> str:
    return f"path+file://{manifest_path.parent}#{package_name}@0.1.0"


@pytest.fixture
def workspace(tmp_path: Path) -> dict[str, Any]:
    app_root = tmp_path / "apps" / "calculator"
    app_manifest = write_manifest(app_root, "calculator")
    (app_root / "src").mkdir()
    (app_root / "src" / "main.rs").write_text("fn main() {}\n", encoding="utf-8")

    ui_root = tmp_path / "crates" / "slopos-ui"
    ui_manifest = write_manifest(ui_root, "slopos-ui")

    return {
        "root": tmp_path,
        "app_root": app_root,
        "app_manifest": app_manifest,
        "app_id": package_identifier(app_manifest, "calculator"),
        "ui_root": ui_root,
        "ui_manifest": ui_manifest,
        "ui_id": package_identifier(ui_manifest, "slopos-ui"),
    }


def make_metadata(
    workspace: dict[str, Any],
    app_dependencies: list[dict[str, Any]] | None = None,
    resolved_app_dependencies: list[dict[str, Any]] | None = None,
    extra_packages: list[dict[str, Any]] | None = None,
    workspace_members: list[str] | None = None,
    extra_nodes: list[dict[str, Any]] | None = None,
    resolution: Any = MISSING_RESOLUTION,
) -> dict[str, Any]:
    packages = [
        make_package(
            workspace["app_id"],
            "calculator",
            workspace["app_manifest"],
            app_dependencies,
            targets=[
                {"src_path": str(workspace["app_root"] / "src" / "main.rs")}
            ],
        ),
        make_package(workspace["ui_id"], "slopos-ui", workspace["ui_manifest"]),
    ]
    packages.extend(extra_packages or [])

    members = workspace_members
    if members is None:
        members = [workspace["app_id"], workspace["ui_id"]]

    nodes = [
        {
            "id": workspace["app_id"],
            "deps": resolved_app_dependencies or [],
        },
        {"id": workspace["ui_id"], "deps": []},
    ]
    nodes.extend(extra_nodes or [])

    metadata: dict[str, Any] = {
        "packages": packages,
        "workspace_members": members,
    }
    if resolution is MISSING_RESOLUTION:
        metadata["resolve"] = {"nodes": nodes}
    elif resolution is not MISSING_RESOLUTION:
        metadata["resolve"] = resolution
    return metadata


def test_accepts_app_with_direct_slopos_ui_dependency(
    workspace: dict[str, Any],
) -> None:
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui", rename="slopos_ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    assert check_architecture(metadata, workspace["root"]) == []


def test_accepts_app_with_direct_slopos_appkit_dependency(
    workspace: dict[str, Any],
) -> None:
    appkit_manifest = write_manifest(
        workspace["root"] / "crates" / "slopos-appkit", "slopos-appkit"
    )
    appkit_id = package_identifier(appkit_manifest, "slopos-appkit")
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-appkit")],
        [make_resolved_dependency(appkit_id, "slopos_appkit")],
        extra_packages=[
            make_package(appkit_id, "slopos-appkit", appkit_manifest)
        ],
        workspace_members=[workspace["app_id"], workspace["ui_id"], appkit_id],
        extra_nodes=[{"id": appkit_id, "deps": []}],
    )

    assert check_architecture(metadata, workspace["root"]) == []


@pytest.mark.parametrize(
    ("dependency_options", "resolved_kind", "resolved_target"),
    [
        pytest.param({"optional": True}, None, None, id="optional"),
        pytest.param({"kind": "dev"}, "dev", None, id="development"),
        pytest.param({"kind": "build"}, "build", None, id="build"),
        pytest.param(
            {"target": 'cfg(target_os = "windows")'},
            None,
            'cfg(target_os = "windows")',
            id="platform-specific",
        ),
    ],
)
def test_rejects_non_runtime_slopos_dependency_as_ui_requirement(
    workspace: dict[str, Any],
    dependency_options: dict[str, Any],
    resolved_kind: str | None,
    resolved_target: str | None,
) -> None:
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui", **dependency_options)],
        [
            make_resolved_dependency(
                workspace["ui_id"],
                "slopos_ui",
                kind=resolved_kind,
                target=resolved_target,
            )
        ],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("runtime slopos-ui" in error for error in errors), errors


@pytest.mark.parametrize(
    "missing_field", ["name", "rename", "kind", "optional", "target"]
)
def test_fails_closed_on_incomplete_declared_dependency_metadata(
    workspace: dict[str, Any], missing_field: str
) -> None:
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )
    del metadata["packages"][0]["dependencies"][0][missing_field]

    errors = check_architecture(metadata, workspace["root"])

    assert any("invalid package dependency entry" in error for error in errors), errors


@pytest.mark.parametrize("missing_field", ["dep_kinds", "kind", "target"])
def test_fails_closed_on_incomplete_resolved_dependency_metadata(
    workspace: dict[str, Any], missing_field: str
) -> None:
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )
    resolved_dependency = metadata["resolve"]["nodes"][0]["deps"][0]
    if missing_field == "dep_kinds":
        del resolved_dependency[missing_field]
    else:
        del resolved_dependency["dep_kinds"][0][missing_field]

    errors = check_architecture(metadata, workspace["root"])

    assert any("invalid dependency" in error for error in errors), errors


def test_fails_closed_when_workspace_member_has_no_resolution_node(
    workspace: dict[str, Any],
) -> None:
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )
    metadata["resolve"]["nodes"] = [
        node
        for node in metadata["resolve"]["nodes"]
        if node["id"] != workspace["ui_id"]
    ]

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "workspace member" in error and "resolution nodes" in error
        for error in errors
    )


def test_rejects_resolution_node_without_a_package_identity(
    workspace: dict[str, Any],
) -> None:
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
        extra_nodes=[{"id": "path+file:///ghost#ghost@0.1.0", "deps": []}],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("package absent from metadata" in error for error in errors), errors


def test_rejects_renamed_forbidden_resolved_package_identity(
    workspace: dict[str, Any],
) -> None:
    gtk_id = "registry+https://example.invalid/index#gtk@0.18.2"
    gtk_package = make_package(
        gtk_id, "gtk", workspace["root"] / "registry" / "gtk" / "Cargo.toml"
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [
            make_resolved_dependency(workspace["ui_id"], "slopos_ui"),
            make_resolved_dependency(gtk_id, "raw_widgets"),
        ],
        extra_packages=[gtk_package],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("gtk" in error.lower() for error in errors), errors


def test_rejects_registry_package_that_impersonates_slopos_ui(
    workspace: dict[str, Any],
) -> None:
    impostor_id = "registry+https://example.invalid/index#slopos-ui@9.9.9"
    impostor_manifest = workspace["root"] / "registry" / "slopos-ui" / "Cargo.toml"
    impostor_package = make_package(impostor_id, "slopos-ui", impostor_manifest)
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui", rename="fake_ui")],
        [make_resolved_dependency(impostor_id, "fake_ui")],
        extra_packages=[impostor_package],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("runtime slopos-ui" in error for error in errors), errors


def test_does_not_pair_required_registry_alias_with_optional_local_ui_edge(
    workspace: dict[str, Any],
) -> None:
    registry_id = "registry+https://example.invalid/index#slopos-ui@9.9.9"
    registry_manifest = workspace["root"] / "registry" / "slopos-ui" / "Cargo.toml"
    registry_package = make_package(
        registry_id,
        "slopos-ui",
        registry_manifest,
        source="registry+https://example.invalid/index",
    )
    metadata = make_metadata(
        workspace,
        [
            make_dependency("slopos-ui"),
            make_dependency("slopos-ui", rename="optional_local_ui", optional=True),
        ],
        [
            make_resolved_dependency(registry_id, "slopos_ui"),
            make_resolved_dependency(workspace["ui_id"], "optional_local_ui"),
        ],
        extra_packages=[registry_package],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("runtime slopos-ui" in error for error in errors), errors


def test_does_not_accept_symlinked_first_party_ui_crate(
    workspace: dict[str, Any],
) -> None:
    real_ui_root = workspace["root"] / "vendor" / "slopos-ui"
    real_ui_root.parent.mkdir(parents=True)
    workspace["ui_root"].rename(real_ui_root)
    workspace["ui_root"].symlink_to(real_ui_root, target_is_directory=True)
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("runtime slopos-ui" in error for error in errors), errors


def test_rejects_declared_optional_forbidden_dependency(
    workspace: dict[str, Any],
) -> None:
    metadata = make_metadata(
        workspace,
        [
            make_dependency("slopos-ui"),
            make_dependency("pango", rename="optional_pango", optional=True),
        ],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("pango" in error.lower() for error in errors), errors


def test_rejects_app_without_direct_slopos_ui_dependency(
    workspace: dict[str, Any],
) -> None:
    errors = check_architecture(make_metadata(workspace), workspace["root"])

    assert any("slopos-ui" in error for error in errors), errors


def test_rejects_raw_rust_ui_import(workspace: dict[str, Any]) -> None:
    (workspace["app_root"] / "src" / "main.rs").write_text(
        "use gdk::prelude::*;\nuse r#gtk::Button;\n", encoding="utf-8"
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("gdk" in error.lower() for error in errors), errors
    assert any("gtk" in error.lower() for error in errors), errors


def test_ignores_raw_ui_references_in_rust_comments_and_strings(
    workspace: dict[str, Any],
) -> None:
    (workspace["app_root"] / "src" / "main.rs").write_text(
        "// gtk::Button is only mentioned in documentation.\n"
        "/* gdk::Display /* nested pango::Font */ remains a comment. */\n"
        'const EXAMPLE: &str = "gtk::Button and pango::Font";\n'
        'const RAW: &str = r##"gdk::Display"##;\n'
        "const QUOTE: char = '\"';\n"
        "fn borrowed<'a>(value: &'a str) -> &'a str { value }\n"
        "fn main() {}\n",
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    assert check_architecture(metadata, workspace["root"]) == []


def test_ignores_ui_names_inside_unicode_rust_identifiers(
    workspace: dict[str, Any],
) -> None:
    (workspace["app_root"] / "src" / "main.rs").write_text(
        "use gtké::SuffixMarker;\n"
        "use égtk::PrefixMarker;\n"
        "use gtk\u0301::CombiningMarkMarker;\n",
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    assert check_architecture(metadata, workspace["root"]) == []


def test_rejects_app_local_css(workspace: dict[str, Any]) -> None:
    (workspace["app_root"] / "theme.css").write_text(
        "button { color: red; }\n", encoding="utf-8"
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any(".css" in error.lower() for error in errors), errors


def test_rejects_cargo_target_source_outside_app_package(
    workspace: dict[str, Any],
) -> None:
    external_source = workspace["root"] / "shared" / "external.rs"
    external_source.parent.mkdir()
    external_source.write_text("use gtk::Button;\n", encoding="utf-8")
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )
    metadata["packages"][0]["targets"][0]["src_path"] = str(external_source)

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "target source escapes the app package root" in error for error in errors
    )
    assert any("gtk" in error.lower() for error in errors), errors


def test_rejects_cargo_target_source_symlink_outside_app_package(
    workspace: dict[str, Any],
) -> None:
    external_source = workspace["root"] / "apps" / "shared" / "external.rs"
    external_source.parent.mkdir()
    external_source.write_text("use gtk::Button;\n", encoding="utf-8")
    linked_source = workspace["app_root"] / "src" / "linked.rs"
    linked_source.symlink_to(external_source)
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )
    metadata["packages"][0]["targets"][0]["src_path"] = str(linked_source)

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "target source resolves outside the app package root" in error
        for error in errors
    ), errors


def test_rejects_external_rust_path_attribute_source(
    workspace: dict[str, Any],
) -> None:
    external_source = workspace["root"] / "shared" / "external.rs"
    external_source.parent.mkdir()
    external_source.write_text("use gtk::Button;\n", encoding="utf-8")
    (workspace["app_root"] / "src" / "main.rs").write_text(
        '#[path = "../../../shared/external.rs"]\nmod external;\n',
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "Rust path attribute source escapes the app package root" in error
        for error in errors
    ), errors


def test_rejects_external_rust_include_source(workspace: dict[str, Any]) -> None:
    external_source = workspace["root"] / "shared" / "external.rs"
    external_source.parent.mkdir()
    external_source.write_text("use gtk::Button;\n", encoding="utf-8")
    (workspace["app_root"] / "src" / "main.rs").write_text(
        'include!("../../../shared/external.rs");\n', encoding="utf-8"
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "Rust include! source escapes the app package root" in error
        for error in errors
    ), errors


def test_rejects_unresolved_rust_include_expression(workspace: dict[str, Any]) -> None:
    (workspace["app_root"] / "src" / "main.rs").write_text(
        'include!(concat!("generated/", "external.rs"));\n', encoding="utf-8"
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "cannot statically verify Rust include! source" in error for error in errors
    ), errors


def test_accepts_app_local_rust_include_and_path_sources(
    workspace: dict[str, Any],
) -> None:
    (workspace["app_root"] / "src" / "helper.rs").write_text(
        "const INCLUDED_VALUE: u8 = 1;\n", encoding="utf-8"
    )
    (workspace["app_root"] / "src" / "nested.rs").write_text(
        "pub const NESTED_VALUE: u8 = 2;\n", encoding="utf-8"
    )
    (workspace["app_root"] / "src" / "main.rs").write_text(
        'include!("helper.rs");\n#[path = "nested.rs"]\nmod nested;\n',
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    assert check_architecture(metadata, workspace["root"]) == []


def test_checks_rust_include_sources_without_rs_extension(
    workspace: dict[str, Any],
) -> None:
    (workspace["app_root"] / "src" / "raw-ui.inc").write_text(
        "use gtk::Button;\n", encoding="utf-8"
    )
    (workspace["app_root"] / "src" / "main.rs").write_text(
        'include!("raw-ui.inc");\n', encoding="utf-8"
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("raw Rust UI reference to 'gtk'" in error for error in errors), errors


def test_checks_rust_path_sources_without_rs_extension(
    workspace: dict[str, Any],
) -> None:
    (workspace["app_root"] / "src" / "raw-ui.inc").write_text(
        "use pango::Font;\n", encoding="utf-8"
    )
    (workspace["app_root"] / "src" / "main.rs").write_text(
        '#[path = "raw-ui.inc"]\nmod raw_ui;\n', encoding="utf-8"
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("raw Rust UI reference to 'pango'" in error for error in errors), errors


def test_accepts_path_source_inside_inline_module_in_root_file(
    workspace: dict[str, Any],
) -> None:
    nested_source = (
        workspace["app_root"] / "src" / "inline" / "inner" / "nested.rs"
    )
    nested_source.parent.mkdir(parents=True)
    nested_source.write_text("pub const VALUE: u8 = 1;\n", encoding="utf-8")
    (workspace["app_root"] / "src" / "main.rs").write_text(
        'mod inline {\n    mod inner {\n        #[path = "nested.rs"]\n'
        "        mod nested;\n    }\n}\n",
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    assert check_architecture(metadata, workspace["root"]) == []


def test_checks_child_path_relative_to_inline_path_override(
    workspace: dict[str, Any],
) -> None:
    nested_source = workspace["app_root"] / "src" / "thread_files" / "tls.rs"
    nested_source.parent.mkdir(parents=True)
    nested_source.write_text("use gtk::Button;\n", encoding="utf-8")
    (workspace["app_root"] / "src" / "main.rs").write_text(
        '#[path = "thread_files"]\n'
        "mod thread {\n"
        '    #[path = "tls.rs"]\n'
        "    mod local_data;\n"
        "}\n",
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("raw Rust UI reference to 'gtk'" in error for error in errors), errors
    assert not any("Rust path attribute source is not a file" in error for error in errors)


def test_rejects_conditional_inline_path_with_inactive_escape(
    workspace: dict[str, Any],
) -> None:
    external_source = workspace["root"] / "shared" / "external.rs"
    external_source.parent.mkdir()
    external_source.write_text("use gtk::Button;\n", encoding="utf-8")
    decoy_source = workspace["app_root"] / "shared" / "external.rs"
    decoy_source.parent.mkdir()
    decoy_source.write_text("pub const VALUE: u8 = 1;\n", encoding="utf-8")
    (workspace["app_root"] / "src" / "main.rs").write_text(
        '#[cfg_attr(target_os = "windows", path = "deep/dir")]\n'
        "mod feature {\n"
        '    #[path = "../../../shared/external.rs"]\n'
        "    mod hidden;\n"
        "}\n",
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "cannot statically verify conditional Rust path attribute" in error
        for error in errors
    ), errors


def test_accepts_path_source_inside_inline_module_in_non_root_file(
    workspace: dict[str, Any],
) -> None:
    nested_source = (
        workspace["app_root"]
        / "src"
        / "outer"
        / "inline"
        / "inner"
        / "nested.rs"
    )
    nested_source.parent.mkdir(parents=True)
    nested_source.write_text("pub const VALUE: u8 = 1;\n", encoding="utf-8")
    outer_source = workspace["app_root"] / "src" / "outer.rs"
    outer_source.write_text(
        'mod inline {\n    mod inner {\n        #[path = "nested.rs"]\n'
        "        mod nested;\n    }\n}\n",
        encoding="utf-8",
    )
    (workspace["app_root"] / "src" / "main.rs").write_text(
        "mod outer;\n", encoding="utf-8"
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    assert check_architecture(metadata, workspace["root"]) == []


def test_rejects_inline_path_override_that_escapes_app_package(
    workspace: dict[str, Any],
) -> None:
    (workspace["app_root"] / "src" / "main.rs").write_text(
        '#[path = "../../../../shared"]\nmod external {\n    mod hidden;\n}\n',
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "Rust inline module path escapes the app package root" in error
        for error in errors
    ), errors


def test_rejects_inline_path_source_escape_from_actual_module_directory(
    workspace: dict[str, Any],
) -> None:
    external_source = workspace["root"] / "shared" / "external.rs"
    external_source.parent.mkdir()
    external_source.write_text("use gtk::Button;\n", encoding="utf-8")
    (workspace["app_root"] / "src" / "main.rs").write_text(
        'mod inline {\n'
        '    #[path = "../../../../shared/external.rs"]\n'
        "    mod external;\n}\n",
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "Rust path attribute source escapes the app package root" in error
        for error in errors
    ), errors


def test_rejects_symlink_escape_from_inline_path_override(
    workspace: dict[str, Any],
) -> None:
    external_source = workspace["root"] / "apps" / "shared" / "external.rs"
    external_source.parent.mkdir()
    external_source.write_text("use gtk::Button;\n", encoding="utf-8")
    nested_directory = workspace["app_root"] / "src" / "inline"
    nested_directory.mkdir()
    (nested_directory / "hidden.rs").symlink_to(external_source)
    (workspace["app_root"] / "src" / "main.rs").write_text(
        '#[path = "inline"]\n'
        "mod inline {\n"
        '    #[path = "hidden.rs"]\n'
        "    mod hidden;\n"
        "}\n",
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any(
        "Rust path attribute source escapes the app package root" in error
        for error in errors
    ), errors


def test_ignores_path_and_include_text_in_comments_and_strings(
    workspace: dict[str, Any],
) -> None:
    (workspace["app_root"] / "src" / "main.rs").write_text(
        '// include!("../../../shared/external.rs"); #[path = "missing.rs"]\n'
        'const EXAMPLE: &str = "include!(\\\"../../../shared/external.rs\\\")";\n',
        encoding="utf-8",
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    assert check_architecture(metadata, workspace["root"]) == []


def test_ignores_non_app_legacy_crate(workspace: dict[str, Any]) -> None:
    legacy_root = workspace["root"] / "crates" / "slopos-shell"
    legacy_manifest = write_manifest(legacy_root, "slopos-shell")
    (legacy_root / "src").mkdir()
    (legacy_root / "src" / "lib.rs").write_text(
        "use gtk::prelude::*;\n", encoding="utf-8"
    )

    gtk_id = "registry+https://example.invalid/index#gtk@0.18.2"
    gtk_package = make_package(
        gtk_id, "gtk", workspace["root"] / "registry" / "gtk" / "Cargo.toml"
    )
    legacy_id = package_identifier(legacy_manifest, "slopos-shell")
    legacy_package = make_package(
        legacy_id, "slopos-shell", legacy_manifest, [make_dependency("gtk")]
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
        extra_packages=[legacy_package, gtk_package],
        workspace_members=[workspace["app_id"], workspace["ui_id"], legacy_id],
        extra_nodes=[
            {
                "id": legacy_id,
                "deps": [make_resolved_dependency(gtk_id, "gtk")],
            }
        ],
    )

    assert check_architecture(metadata, workspace["root"]) == []


def test_ledger_ruling_rejects_app_manifest_missing_from_cargo_metadata(
    workspace: dict[str, Any],
) -> None:
    orphan_manifest = write_manifest(
        workspace["root"] / "apps" / "unregistered", "unregistered"
    )
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("apps/unregistered/Cargo.toml" in error for error in errors), errors


def test_ledger_ruling_rejects_app_manifest_missing_from_workspace_members(
    workspace: dict[str, Any],
) -> None:
    orphan_manifest = write_manifest(
        workspace["root"] / "apps" / "unregistered", "unregistered"
    )
    orphan_id = package_identifier(orphan_manifest, "unregistered")
    orphan_package = make_package(orphan_id, "unregistered", orphan_manifest)
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
        extra_packages=[orphan_package],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("apps/unregistered/Cargo.toml" in error for error in errors), errors


@pytest.mark.parametrize("symlink_kind", ["directory", "manifest"])
def test_ledger_ruling_rejects_app_symlink_that_escapes_apps(
    workspace: dict[str, Any], symlink_kind: str
) -> None:
    outside_root = workspace["root"] / "crates" / "escaped-app"
    outside_manifest = write_manifest(outside_root, "escaped-app")
    outside_id = package_identifier(outside_manifest, "escaped-app")
    if symlink_kind == "directory":
        (workspace["root"] / "apps" / "escaped-app").symlink_to(
            outside_root, target_is_directory=True
        )
    else:
        symlink_root = workspace["root"] / "apps" / "escaped-app"
        symlink_root.mkdir()
        (symlink_root / "Cargo.toml").symlink_to(outside_manifest)
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
        extra_packages=[
            make_package(
                outside_id,
                "escaped-app",
                outside_manifest,
                [make_dependency("slopos-ui")],
            )
        ],
        workspace_members=[workspace["app_id"], workspace["ui_id"], outside_id],
        extra_nodes=[
            {
                "id": outside_id,
                "deps": [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
            }
        ],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("symlink" in error.lower() for error in errors), errors


def test_rejects_symlinked_apps_root(workspace: dict[str, Any]) -> None:
    apps_root = workspace["root"] / "apps"
    real_apps_root = workspace["root"] / "apps-real"
    apps_root.rename(real_apps_root)
    apps_root.symlink_to(real_apps_root, target_is_directory=True)
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("symlinked application root" in error for error in errors), errors


def test_rejects_broken_app_tree_symlink(workspace: dict[str, Any]) -> None:
    broken_link = workspace["app_root"] / "src" / "missing.rs"
    broken_link.symlink_to(workspace["app_root"] / "src" / "does-not-exist.rs")
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("broken app-tree symlink" in error for error in errors), errors


def test_scans_source_reached_through_in_tree_file_symlink(
    workspace: dict[str, Any],
) -> None:
    shared_source = workspace["root"] / "apps" / "shared" / "hidden.rs"
    shared_source.parent.mkdir()
    shared_source.write_text("use gtk::Button;\n", encoding="utf-8")
    linked_source = workspace["app_root"] / "src" / "linked.rs"
    linked_source.symlink_to(shared_source)
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )

    errors = check_architecture(metadata, workspace["root"])

    assert any("gtk" in error.lower() for error in errors), errors


def test_ledger_ruling_fails_closed_without_workspace_membership_metadata(
    workspace: dict[str, Any],
) -> None:
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
    )
    del metadata["workspace_members"]

    errors = check_architecture(metadata, workspace["root"])

    assert any("workspace_members" in error for error in errors), errors


@pytest.mark.parametrize(
    "resolution",
    [
        pytest.param(MISSING_RESOLUTION, id="missing"),
        pytest.param(None, id="null"),
        pytest.param([], id="not-an-object"),
        pytest.param({}, id="missing-nodes"),
        pytest.param({"nodes": "invalid"}, id="nodes-not-a-list"),
        pytest.param({"nodes": [{"id": "app-without-deps"}]}, id="node-incomplete"),
    ],
)
def test_ledger_ruling_fails_closed_on_missing_or_invalid_resolution_metadata(
    workspace: dict[str, Any], resolution: Any
) -> None:
    metadata = make_metadata(
        workspace,
        [make_dependency("slopos-ui")],
        [make_resolved_dependency(workspace["ui_id"], "slopos_ui")],
        resolution=resolution,
    )
    if resolution is MISSING_RESOLUTION:
        del metadata["resolve"]

    errors = check_architecture(metadata, workspace["root"])

    assert any("resolve" in error.lower() for error in errors), errors


def test_cli_returns_one_for_architecture_violation(tmp_path: Path) -> None:
    cargo_script = (
        "printf '%s\\n' '{\"packages\":[],\"workspace_members\":[],"
        "\"resolve\":{\"nodes\":[]}}'"
    )

    result = run_checker_cli(tmp_path, cargo_script)

    assert result.returncode == 1
    assert "Architecture check failed" in result.stderr


def test_cli_returns_two_when_cargo_metadata_fails(tmp_path: Path) -> None:
    result = run_checker_cli(
        tmp_path, "printf 'fixture metadata failure\\n' >&2\nexit 23"
    )

    assert result.returncode == 2
    assert "fixture metadata failure" in result.stderr


def test_cli_returns_two_for_invalid_cargo_metadata_json(tmp_path: Path) -> None:
    result = run_checker_cli(tmp_path, "printf '%s\\n' 'not-json'")

    assert result.returncode == 2
    assert "invalid JSON" in result.stderr


def test_cli_returns_zero_and_disclaims_missing_app_coverage(tmp_path: Path) -> None:
    cargo_script = (
        "printf '%s\\n' '{\"packages\":[{\"id\":\"fixture\","
        "\"name\":\"fixture\",\"manifest_path\":\"/tmp/Cargo.toml\","
        "\"source\":null,\"dependencies\":[],\"targets\":[]}],"
        "\"workspace_members\":[\"fixture\"],"
        "\"resolve\":{\"nodes\":[{\"id\":\"fixture\",\"deps\":[]}]}}'"
    )

    result = run_checker_cli(tmp_path, cargo_script)

    assert result.returncode == 0
    assert "app conformance not demonstrated" in result.stdout
