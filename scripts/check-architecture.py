from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


FORBIDDEN_UI_PACKAGES = frozenset({"gtk", "gdk", "pango"})
SLOPOS_UI_PACKAGES = frozenset({"slopos-ui", "slopos-appkit"})
RUST_UI_REFERENCE = re.compile(
    r"(?<![A-Za-z0-9_])(?:use\s+(?:::)?|extern\s+crate\s+)"
    r"(?:r#)?(?P<import>gtk|gdk|pango)(?![A-Za-z0-9_])"
    r"|(?<![A-Za-z0-9_])(?P<path>gtk|gdk|pango)\s*::"
)
RUST_RAW_STRING_START = re.compile(r'r(?P<hashes>#+)?"')
RUST_INCLUDE_MACRO = re.compile(r"(?<![A-Za-z0-9_])include\s*!")
RUST_ATTRIBUTE_START = re.compile(r"#\s*!?\s*\[")
RUST_ATTRIBUTE_PATH = re.compile(r"(?<![\w:])path\s*=")
RUST_MODULE_DECLARATION = re.compile(r"(?<![\w:])mod(?!\w)")


def normalize_package_name(package_name: str) -> str:
    return package_name.casefold().replace("_", "-")


def normalize_dependency_alias(alias: str) -> str:
    return alias.replace("-", "_")


def absolute_path(path: Path, root: Path) -> Path:
    candidate = path if path.is_absolute() else root / path
    return Path(os.path.abspath(candidate))


def workspace_app_packages(
    metadata: dict[str, Any], root: Path
) -> list[dict[str, Any]]:
    packages_value = metadata.get("packages", [])
    members_value = metadata.get("workspace_members", [])
    if not isinstance(packages_value, list) or not isinstance(members_value, list):
        return []

    member_ids = {member for member in members_value if isinstance(member, str)}
    apps_root = absolute_path(root / "apps", root)
    app_packages: list[dict[str, Any]] = []
    for package in packages_value:
        if not isinstance(package, dict) or package.get("id") not in member_ids:
            continue
        manifest_path = package.get("manifest_path")
        if not isinstance(manifest_path, str):
            continue
        package_root = absolute_path(Path(manifest_path), root).parent
        if package_root.is_relative_to(apps_root):
            app_packages.append(package)
    return app_packages


def app_cargo_manifests(root: Path) -> list[Path]:
    apps_root = absolute_path(root / "apps", root)
    if apps_root.is_symlink() or not apps_root.is_dir():
        return []
    return sorted(
        (
            absolute_path(manifest, root)
            for manifest in apps_root.rglob("Cargo.toml")
            if manifest.is_file()
        ),
        key=str,
    )


def app_target_source_violations(
    package: dict[str, Any], package_root: Path, root: Path
) -> list[str]:
    errors: set[str] = set()
    package_root = absolute_path(package_root, root)
    try:
        resolved_package_root = package_root.resolve(strict=True)
    except (OSError, RuntimeError):
        return [f"{relative_path(package_root, root)}: cannot resolve app package root"]

    for target in package["targets"]:
        target_source = absolute_path(Path(target["src_path"]), package_root)
        displayed_path = relative_path(target_source, root)
        if not target_source.is_relative_to(package_root):
            errors.add(
                f"{displayed_path}: Cargo target source escapes the app package root"
            )
            continue
        if not target_source.is_file():
            errors.add(f"{displayed_path}: Cargo target source is not a readable file")
            continue
        try:
            resolved_target_source = target_source.resolve(strict=True)
        except (OSError, RuntimeError):
            errors.add(f"{displayed_path}: cannot resolve Cargo target source")
            continue
        if not resolved_target_source.is_relative_to(resolved_package_root):
            errors.add(f"{displayed_path}: Cargo target source resolves outside "
                       "the app package root")
    return sorted(errors)


def direct_dependency_names(
    package: dict[str, Any],
    resolve_node: dict[str, Any] | None,
    packages_by_id: dict[str, dict[str, Any]],
) -> set[str]:
    names: set[str] = set()

    dependencies = package.get("dependencies", [])
    if isinstance(dependencies, list):
        for dependency in dependencies:
            if not isinstance(dependency, dict):
                continue
            dependency_name = dependency.get("name")
            if isinstance(dependency_name, str):
                names.add(normalize_package_name(dependency_name))

    if resolve_node is not None:
        resolved_dependencies = resolve_node.get("deps", [])
        if isinstance(resolved_dependencies, list):
            for dependency in resolved_dependencies:
                if not isinstance(dependency, dict):
                    continue
                package_id = dependency.get("pkg")
                resolved_package = packages_by_id.get(package_id)
                if resolved_package is None:
                    continue
                package_name = resolved_package.get("name")
                if isinstance(package_name, str):
                    names.add(normalize_package_name(package_name))

    return names


def direct_runtime_slopos_dependencies(
    package: dict[str, Any],
    resolve_node: dict[str, Any] | None,
    packages_by_id: dict[str, dict[str, Any]],
    slopos_ui_package_ids: set[str],
) -> set[str]:
    declared_aliases: dict[str, str] = {}
    dependencies = package.get("dependencies", [])
    if isinstance(dependencies, list):
        for dependency in dependencies:
            if not isinstance(dependency, dict):
                continue
            dependency_name = dependency.get("name")
            if (
                not isinstance(dependency_name, str)
                or "kind" not in dependency
                or dependency.get("kind") is not None
                or "optional" not in dependency
                or dependency.get("optional") is not False
                or "target" not in dependency
                or dependency.get("target") is not None
            ):
                continue
            normalized_name = normalize_package_name(dependency_name)
            if normalized_name in SLOPOS_UI_PACKAGES:
                dependency_alias = dependency["rename"] or dependency_name
                declared_aliases[normalize_dependency_alias(dependency_alias)] = (
                    normalized_name
                )

    if resolve_node is None:
        return set()

    names: set[str] = set()
    dependencies = resolve_node.get("deps", [])
    if not isinstance(dependencies, list):
        return names
    for dependency in dependencies:
        if not isinstance(dependency, dict):
            continue
        package_id = dependency.get("pkg")
        dependency_alias = dependency.get("name")
        if not isinstance(dependency_alias, str):
            continue
        expected_package_name = declared_aliases.get(
            normalize_dependency_alias(dependency_alias)
        )
        if expected_package_name is None:
            continue
        if package_id not in slopos_ui_package_ids:
            continue
        resolved_package = packages_by_id.get(package_id)
        if resolved_package is None:
            continue
        package_name = resolved_package.get("name")
        if not isinstance(package_name, str):
            continue
        normalized_name = normalize_package_name(package_name)
        if normalized_name != expected_package_name:
            continue
        dependency_kinds = dependency.get("dep_kinds", [])
        if not isinstance(dependency_kinds, list) or not dependency_kinds:
            continue
        if any(
            isinstance(dependency_kind, dict)
            and "kind" in dependency_kind
            and "target" in dependency_kind
            and dependency_kind.get("kind") is None
            and dependency_kind.get("target") is None
            for dependency_kind in dependency_kinds
        ):
            names.add(normalized_name)
    return names


def first_party_slopos_ui_package_ids(
    packages_by_id: dict[str, dict[str, Any]],
    workspace_member_ids: set[str],
    root: Path,
) -> set[str]:
    expected_manifests = {
        package_name: absolute_path(root / "crates" / package_name / "Cargo.toml", root)
        for package_name in SLOPOS_UI_PACKAGES
    }
    package_ids: set[str] = set()
    for package_id, package in packages_by_id.items():
        package_name = normalize_package_name(package["name"])
        if (
            package_name not in expected_manifests
            or package_id not in workspace_member_ids
        ):
            continue
        expected_manifest = expected_manifests[package_name]
        manifest_path = absolute_path(Path(package["manifest_path"]), root)
        if (
            manifest_path == expected_manifest
            and package.get("source") is None
            and not path_has_symlink_component(expected_manifest, root)
        ):
            package_ids.add(package_id)
    return package_ids


def path_has_symlink_component(path: Path, root: Path) -> bool:
    root_path = absolute_path(root, root)
    try:
        relative_path_value = absolute_path(path, root).relative_to(root_path)
    except ValueError:
        return True
    current_path = root_path
    for component in relative_path_value.parts:
        current_path /= component
        if current_path.is_symlink():
            return True
    return False


def app_symlink_violations(root: Path) -> list[str]:
    apps_root = absolute_path(root / "apps", root)
    if apps_root.is_symlink():
        return ["apps/: a symlinked application root is not supported"]
    if not apps_root.is_dir():
        return []

    errors: set[str] = set()
    apps_root_resolved = apps_root.resolve()
    for path in apps_root.rglob("*"):
        if not path.is_symlink():
            continue
        displayed_path = relative_path(path, root)
        try:
            target = path.resolve(strict=False)
        except (OSError, RuntimeError):
            errors.add(f"{displayed_path}: cannot resolve app-tree symlink")
            continue
        if not path.exists():
            errors.add(f"{displayed_path}: broken app-tree symlink")
        if not target.is_relative_to(apps_root_resolved):
            errors.add(f"{displayed_path}: app-tree symlink escapes apps/")
        if path.is_dir():
            errors.add(f"{displayed_path}: symlinked app directories are not supported")
        if path.name == "Cargo.toml":
            errors.add(f"{displayed_path}: symlinked Cargo manifests are not supported")
    return sorted(errors)


def rust_code_without_comments_or_strings(source: str) -> str:
    characters = list(source)
    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index + 2)
            end = len(source) if end == -1 else end
            mask_source_span(characters, source, index, end)
            index = end
            continue
        if source.startswith("/*", index):
            depth = 1
            end = index + 2
            while end < len(source) and depth:
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            mask_source_span(characters, source, index, end)
            index = end
            continue

        raw_string = RUST_RAW_STRING_START.match(source, index)
        if raw_string is not None:
            hashes = raw_string.group("hashes") or ""
            terminator = f'"{hashes}'
            closing_quote = source.find(terminator, raw_string.end())
            end = (
                len(source)
                if closing_quote == -1
                else closing_quote + len(terminator)
            )
            mask_source_span(characters, source, index, end)
            index = end
            continue

        if source[index] == '"':
            end = index + 1
            while end < len(source):
                if source[end] == "\\":
                    end += 2
                elif source[end] == '"':
                    end += 1
                    break
                else:
                    end += 1
            end = min(end, len(source))
            mask_source_span(characters, source, index, end)
            index = end
            continue

        if source[index] == "'":
            end = rust_char_literal_end(source, index)
            if end is not None:
                mask_source_span(characters, source, index, end)
                index = end
                continue
        index += 1
    return "".join(characters)


def mask_source_span(characters: list[str], source: str, start: int, end: int) -> None:
    for position in range(start, end):
        if source[position] not in "\r\n":
            characters[position] = " "


def rust_char_literal_end(source: str, start: int) -> int | None:
    index = start + 1
    if index >= len(source):
        return None
    if source[index] == "\\":
        index += 1
        if index >= len(source):
            return None
        escape = source[index]
        if escape in "nrt0\\'\"":
            index += 1
        elif escape == "x":
            index += 3
        elif escape == "u" and index + 1 < len(source) and source[index + 1] == "{":
            closing_brace = source.find("}", index + 2)
            if closing_brace == -1:
                return None
            index = closing_brace + 1
        else:
            return None
    else:
        if source[index] in "\r\n'":
            return None
        index += 1
    if index < len(source) and source[index] == "'":
        return index + 1
    return None


def rust_string_literal_at(source: str, start: int) -> tuple[str, int] | None:
    raw_string = RUST_RAW_STRING_START.match(source, start)
    if raw_string is not None:
        hashes = raw_string.group("hashes") or ""
        terminator = f'"{hashes}'
        closing_quote = source.find(terminator, raw_string.end())
        if closing_quote == -1:
            return None
        return (
            source[raw_string.end() : closing_quote],
            closing_quote + len(terminator),
        )

    if start >= len(source) or source[start] != '"':
        return None
    end = start + 1
    has_escape = False
    while end < len(source):
        if source[end] == "\\":
            has_escape = True
            end += 2
        elif source[end] == '"':
            if has_escape:
                return None
            return source[start + 1 : end], end + 1
        else:
            end += 1
    return None


def next_rust_token_index(source: str, start: int, limit: int | None = None) -> int:
    end = len(source) if limit is None else limit
    while start < end:
        if source[start].isspace():
            start += 1
        elif source.startswith("//", start):
            newline = source.find("\n", start + 2, end)
            start = end if newline == -1 else newline + 1
        elif source.startswith("/*", start):
            depth = 1
            start += 2
            while start < end and depth:
                if source.startswith("/*", start):
                    depth += 1
                    start += 2
                elif source.startswith("*/", start):
                    depth -= 1
                    start += 2
                else:
                    start += 1
        else:
            break
    return start


def matching_masked_delimiter(
    code: str, opening_index: int, opening: str, closing: str
) -> int | None:
    depth = 0
    for index in range(opening_index, len(code)):
        if code[index] == opening:
            depth += 1
        elif code[index] == closing:
            depth -= 1
            if depth == 0:
                return index
    return None


def rust_inline_module_at(
    code: str, declaration_index: int
) -> tuple[int, int, str] | None:
    declaration = RUST_MODULE_DECLARATION.match(code, declaration_index)
    if declaration is None or code[max(0, declaration.start() - 2) : declaration.start()] == "r#":
        return None

    name_start = next_rust_token_index(code, declaration.end())
    if code.startswith("r#", name_start):
        name_start += 2
    if name_start >= len(code):
        return None
    if not (
        code[name_start] == "_"
        or code[name_start].isalpha()
        or code[name_start].isidentifier()
    ):
        return None

    name_end = name_start + 1
    while name_end < len(code) and is_rust_identifier_continue(code[name_end]):
        name_end += 1
    body_start = next_rust_token_index(code, name_end)
    if body_start >= len(code) or code[body_start] != "{":
        return None
    body_end = matching_masked_delimiter(code, body_start, "{", "}")
    if body_end is None:
        return None
    return body_start, body_end, code[name_start:name_end]


def rust_inline_module_after_attributes(
    code: str, start: int
) -> tuple[int, int, str] | None:
    cursor = next_rust_token_index(code, start)
    while cursor < len(code) and code[cursor] == "#":
        attribute_open = next_rust_token_index(code, cursor + 1)
        if attribute_open < len(code) and code[attribute_open] == "!":
            attribute_open = next_rust_token_index(code, attribute_open + 1)
        if attribute_open >= len(code) or code[attribute_open] != "[":
            return None
        attribute_end = matching_masked_delimiter(code, attribute_open, "[", "]")
        if attribute_end is None:
            return None
        cursor = next_rust_token_index(code, attribute_end + 1)

    visibility = re.match(r"pub(?!\w)", code[cursor:])
    if visibility is not None:
        cursor = next_rust_token_index(code, cursor + visibility.end())
        if cursor < len(code) and code[cursor] == "(":
            visibility_end = matching_masked_delimiter(code, cursor, "(", ")")
            if visibility_end is None:
                return None
            cursor = next_rust_token_index(code, visibility_end + 1)

    unsafe = re.match(r"unsafe(?!\w)", code[cursor:])
    if unsafe is not None:
        cursor = next_rust_token_index(code, cursor + unsafe.end())

    if RUST_MODULE_DECLARATION.match(code, cursor) is None:
        return None
    return rust_inline_module_at(code, cursor)


def rust_inline_module_blocks(code: str) -> list[tuple[int, int, str]]:
    blocks: list[tuple[int, int, str]] = []

    for declaration in RUST_MODULE_DECLARATION.finditer(code):
        inline_module = rust_inline_module_at(code, declaration.start())
        if inline_module is not None:
            blocks.append(inline_module)

    return blocks


def rust_source_path_references(
    source: str,
) -> list[tuple[str, str | None, tuple[tuple[str, str | None], ...]]]:
    code = rust_code_without_comments_or_strings(source)
    references: list[
        tuple[str, str | None, tuple[tuple[str, str | None], ...]]
    ] = []

    for match in RUST_INCLUDE_MACRO.finditer(code):
        if match.start() > 0 and is_rust_identifier_continue(code[match.start() - 1]):
            continue
        opening_index = next_rust_token_index(source, match.end())
        closing_by_opening = {"(": ")", "[": "]", "{": "}"}
        if opening_index >= len(code) or code[opening_index] not in closing_by_opening:
            references.append(("include!", None, ()))
            continue
        closing = closing_by_opening[code[opening_index]]
        argument_index = next_rust_token_index(source, opening_index + 1)
        literal = rust_string_literal_at(source, argument_index)
        if literal is None:
            references.append(("include!", None, ()))
            continue
        path_value, literal_end = literal
        closing_index = next_rust_token_index(source, literal_end)
        if closing_index < len(code) and code[closing_index] == ",":
            closing_index = next_rust_token_index(source, closing_index + 1)
        macro_end = matching_masked_delimiter(
            code, opening_index, code[opening_index], closing
        )
        if closing_index != macro_end:
            references.append(("include!", None, ()))
            continue
        references.append(("include!", path_value, ()))

    path_attributes: list[
        tuple[int, str | None, tuple[int, int, str] | None, bool]
    ] = []
    for attribute in RUST_ATTRIBUTE_START.finditer(code):
        opening_index = attribute.end() - 1
        closing_index = matching_masked_delimiter(code, opening_index, "[", "]")
        if closing_index is None:
            continue
        is_inner_attribute = "!" in code[attribute.start() : opening_index]
        for path_attribute in RUST_ATTRIBUTE_PATH.finditer(
            code, attribute.end(), closing_index
        ):
            is_conditional = (
                re.search(
                    r"(?<![\w:])cfg_attr(?!\w)",
                    code[attribute.end() : path_attribute.start()],
                )
                is not None
            )
            literal_index = next_rust_token_index(
                source, path_attribute.end(), closing_index
            )
            literal = rust_string_literal_at(source, literal_index)
            inline_module = (
                None
                if is_inner_attribute
                else rust_inline_module_after_attributes(code, closing_index + 1)
            )
            path_attributes.append(
                (
                    path_attribute.start(),
                    None if literal is None else literal[0],
                    inline_module,
                    is_conditional,
                )
            )

    inline_path_values = {
        (body_start, body_end): path_value
        for _, path_value, inline_module, is_conditional in path_attributes
        if inline_module is not None and not is_conditional
        for body_start, body_end, _ in (inline_module,)
    }
    inline_modules = [
        (body_start, body_end, name, inline_path_values.get((body_start, body_end)))
        for body_start, body_end, name in rust_inline_module_blocks(code)
    ]

    def containing_modules(
        position: int,
    ) -> tuple[tuple[str, str | None], ...]:
        return tuple(
            (name, path_value)
            for body_start, body_end, name, path_value in sorted(
                inline_modules, key=lambda block: block[0]
            )
            if body_start < position < body_end
        )

    for position, path_value, inline_module, is_conditional in path_attributes:
        if is_conditional:
            references.append(
                ("conditional path attribute", path_value, containing_modules(position))
            )
            continue
        if inline_module is None:
            references.append(
                ("path attribute", path_value, containing_modules(position))
            )
            continue

        references.append(
            (
                "inline module path attribute",
                path_value,
                containing_modules(position),
            )
        )

    return references


def rust_path_attribute_base(
    source_path: Path,
    inline_modules: tuple[tuple[str, str | None], ...],
    inline_module_path: bool,
) -> Path:
    base_path = source_path.parent
    if inline_module_path or inline_modules:
        if source_path.name not in {"lib.rs", "main.rs", "mod.rs"}:
            base_path /= source_path.stem
        for module_name, path_override in inline_modules:
            base_path /= path_override if path_override is not None else module_name
    return base_path


def rust_compile_time_source_violations(
    source: str, source_path: Path, package_root: Path, root: Path
) -> tuple[list[str], set[Path]]:
    errors: set[str] = set()
    included_sources: set[Path] = set()
    displayed_source = relative_path(source_path, root)
    try:
        resolved_package_root = package_root.resolve(strict=True)
        resolved_source = source_path.resolve(strict=True)
    except (OSError, RuntimeError):
        return [f"{displayed_source}: cannot resolve Rust source path"], set()

    if not resolved_source.is_relative_to(resolved_package_root):
        errors.add(
            f"{displayed_source}: Rust source resolves outside the app package root"
        )

    for source_kind, path_value, inline_modules in rust_source_path_references(source):
        if source_kind == "conditional path attribute":
            errors.add(
                f"{displayed_source}: cannot statically verify conditional Rust "
                "path attribute"
            )
            continue
        if path_value is None:
            errors.add(
                f"{displayed_source}: cannot statically verify Rust "
                f"{source_kind} source"
            )
            continue

        candidate = Path(path_value)
        if not candidate.is_absolute():
            candidate = rust_path_attribute_base(
                source_path,
                inline_modules,
                source_kind == "inline module path attribute",
            ) / candidate

        if source_kind == "inline module path attribute":
            displayed_target = relative_path(candidate, root)
            try:
                resolved_target = candidate.resolve(strict=False)
            except (OSError, RuntimeError):
                errors.add(
                    f"{displayed_source}: Rust inline module path cannot be resolved"
                )
                continue
            if not resolved_target.is_relative_to(resolved_package_root):
                errors.add(
                    f"{displayed_target}: Rust inline module path escapes the app "
                    "package root"
                )
            continue

        displayed_target = relative_path(candidate, root)
        try:
            resolved_target = candidate.resolve(strict=False)
        except (OSError, RuntimeError):
            errors.add(
                f"{displayed_source}: Rust {source_kind} source cannot be resolved"
            )
            continue
        if not resolved_target.is_relative_to(resolved_package_root):
            errors.add(
                f"{displayed_target}: Rust {source_kind} source escapes the app "
                "package root"
            )
        elif not resolved_target.exists():
            errors.add(
                f"{displayed_source}: Rust {source_kind} source cannot be resolved"
            )
        elif not resolved_target.is_file():
            errors.add(
                f"{displayed_target}: Rust {source_kind} source is not a file"
            )
        else:
            included_sources.add(resolved_target)

    return sorted(errors), included_sources


def is_rust_identifier_continue(character: str) -> bool:
    return character == "_" or f"A{character}".isidentifier()


def is_complete_rust_identifier_reference(match: re.Match[str], source: str) -> bool:
    group_name = "import" if match.group("import") is not None else "path"
    start = match.start(group_name)
    end = match.end(group_name)
    return not (
        (start > 0 and is_rust_identifier_continue(source[start - 1]))
        or (end < len(source) and is_rust_identifier_continue(source[end]))
    )


def relative_path(path: Path, root: Path) -> str:
    try:
        absolute_root = absolute_path(root, root)
        return absolute_path(path, root).relative_to(absolute_root).as_posix()
    except ValueError:
        return path.as_posix()


def check_architecture(metadata: dict[str, Any], root: Path) -> list[str]:
    errors: set[str] = set(app_symlink_violations(root))
    packages_value = metadata.get("packages", [])
    if not isinstance(packages_value, list):
        return ["Cargo metadata has no valid packages list"]

    packages: list[dict[str, Any]] = []
    for package in packages_value:
        if (
            not isinstance(package, dict)
            or not isinstance(package.get("id"), str)
            or not isinstance(package.get("name"), str)
            or not isinstance(package.get("manifest_path"), str)
            or "source" not in package
            or (
                package["source"] is not None
                and not isinstance(package["source"], str)
            )
            or not isinstance(package.get("dependencies"), list)
            or not isinstance(package.get("targets"), list)
        ):
            errors.add("Cargo metadata contains an invalid package entry")
            continue
        for dependency in package["dependencies"]:
            if (
                not isinstance(dependency, dict)
                or not isinstance(dependency.get("name"), str)
                or "rename" not in dependency
                or (
                    dependency["rename"] is not None
                    and not isinstance(dependency["rename"], str)
                )
                or not dependency["rename"]
                and dependency["rename"] is not None
                or "kind" not in dependency
                or (
                    dependency["kind"] is not None
                    and not isinstance(dependency["kind"], str)
                )
                or "optional" not in dependency
                or not isinstance(dependency["optional"], bool)
                or "target" not in dependency
                or (
                    dependency["target"] is not None
                    and not isinstance(dependency["target"], str)
                )
            ):
                errors.add(
                    "Cargo metadata contains an invalid package dependency entry"
                )
        for target in package["targets"]:
            if not isinstance(target, dict) or not isinstance(
                target.get("src_path"), str
            ):
                errors.add("Cargo metadata contains an invalid package target entry")
        packages.append(package)
    if errors:
        return sorted(errors)

    packages_by_id: dict[str, dict[str, Any]] = {}
    packages_by_manifest: dict[Path, list[dict[str, Any]]] = {}
    for package in packages:
        package_id = package["id"]
        if package_id in packages_by_id:
            errors.add(f"Cargo metadata repeats package identity {package_id!r}")
        packages_by_id[package_id] = package
        manifest_path = absolute_path(Path(package["manifest_path"]), root)
        packages_by_manifest.setdefault(manifest_path, []).append(package)

    members_value = metadata.get("workspace_members")
    if (
        not isinstance(members_value, list)
        or not members_value
        or any(not isinstance(member, str) for member in members_value)
    ):
        errors.add("Cargo metadata has missing or invalid workspace_members")
        return sorted(errors)
    member_ids = set(members_value)
    for member_id in member_ids:
        if member_id not in packages_by_id:
            errors.add(
                f"workspace member {member_id!r} is missing from Cargo "
                "metadata packages"
            )
    slopos_ui_package_ids = first_party_slopos_ui_package_ids(
        packages_by_id, member_ids, root
    )

    resolve_value = metadata.get("resolve")
    if not isinstance(resolve_value, dict):
        errors.add("Cargo metadata has missing or invalid resolve metadata")
        return sorted(errors)
    resolve_nodes_value = resolve_value.get("nodes")
    if not isinstance(resolve_nodes_value, list):
        errors.add("Cargo metadata has missing or invalid resolve.nodes metadata")
        return sorted(errors)

    resolve_nodes_by_id: dict[str, dict[str, Any]] = {}
    for node in resolve_nodes_value:
        if (
            not isinstance(node, dict)
            or not isinstance(node.get("id"), str)
            or not isinstance(node.get("deps"), list)
        ):
            errors.add(
                "Cargo resolution metadata contains an invalid resolve.nodes entry"
            )
            continue
        node_id = node["id"]
        if node_id in resolve_nodes_by_id:
            errors.add(f"Cargo resolution metadata repeats node identity {node_id!r}")
        if node_id not in packages_by_id:
            errors.add(
                f"Cargo resolution node {node_id!r} references a package absent "
                "from metadata"
            )
        resolve_nodes_by_id[node_id] = node
        for dependency in node["deps"]:
            if (
                not isinstance(dependency, dict)
                or not isinstance(dependency.get("pkg"), str)
                or not isinstance(dependency.get("name"), str)
                or not isinstance(dependency.get("dep_kinds"), list)
                or not dependency["dep_kinds"]
            ):
                errors.add(
                    f"Cargo resolution node {node_id!r} has an invalid dependency entry"
                )
                continue
            if dependency["pkg"] not in packages_by_id:
                errors.add(
                    f"Cargo resolution node {node_id!r} references a package absent "
                    "from metadata"
                )
            for dependency_kind in dependency["dep_kinds"]:
                if (
                    not isinstance(dependency_kind, dict)
                    or "kind" not in dependency_kind
                    or (
                        dependency_kind["kind"] is not None
                        and not isinstance(dependency_kind["kind"], str)
                    )
                    or "target" not in dependency_kind
                    or (
                        dependency_kind["target"] is not None
                        and not isinstance(dependency_kind["target"], str)
                    )
                ):
                    errors.add(
                        f"Cargo resolution node {node_id!r} has an invalid "
                        "dependency kind"
                    )

    for member_id in member_ids:
        if member_id not in resolve_nodes_by_id:
            errors.add(
                f"workspace member {member_id!r} is missing from Cargo resolution nodes"
            )

    if errors:
        return sorted(errors)

    for manifest_path in app_cargo_manifests(root):
        manifest_packages = packages_by_manifest.get(manifest_path, [])
        displayed_path = relative_path(manifest_path, root)
        if not manifest_packages:
            errors.add(
                f"{displayed_path} is not represented in Cargo metadata packages"
            )
        elif not any(package["id"] in member_ids for package in manifest_packages):
            errors.add(
                f"{displayed_path} is not represented by a Cargo workspace member"
            )

    for package in workspace_app_packages(metadata, root):
        package_id = package.get("id")
        package_name = package.get("name", "<unknown>")
        manifest_path = package.get("manifest_path")
        if not isinstance(package_id, str) or not isinstance(manifest_path, str):
            errors.add(f"apps package {package_name!r} has invalid Cargo metadata")
            continue

        package_root = absolute_path(Path(manifest_path), root).parent
        errors.update(app_target_source_violations(package, package_root, root))
        if package_id not in resolve_nodes_by_id:
            errors.add(
                f"{relative_path(package_root, root)} ({package_name}) is missing from "
                "Cargo resolve.nodes"
            )
        direct_names = direct_dependency_names(
            package, resolve_nodes_by_id.get(package_id), packages_by_id
        )
        runtime_slopos_names = direct_runtime_slopos_dependencies(
            package,
            resolve_nodes_by_id.get(package_id),
            packages_by_id,
            slopos_ui_package_ids,
        )
        if not runtime_slopos_names:
            errors.add(
                f"{relative_path(package_root, root)} ({package_name}) must directly "
                "depend on a non-optional, unconditional runtime "
                "slopos-ui or slopos-appkit"
            )

        for forbidden_name in sorted(direct_names & FORBIDDEN_UI_PACKAGES):
            errors.add(
                f"{relative_path(package_root, root)} ({package_name}) directly "
                f"depends on forbidden UI package {forbidden_name!r}"
            )

        target_source_paths = {
            absolute_path(Path(target["src_path"]), package_root)
            for target in package["targets"]
        }
        app_files = set(package_root.rglob("*")) | target_source_paths
        rust_source_files = {
            app_file
            for app_file in app_files
            if app_file.suffix == ".rs" or app_file in target_source_paths
        }
        pending_files = set(app_files)
        scanned_files: set[Path] = set()
        while pending_files:
            app_file = min(pending_files, key=str)
            pending_files.remove(app_file)
            if app_file in scanned_files:
                continue
            scanned_files.add(app_file)
            if not app_file.is_file():
                continue
            displayed_path = relative_path(app_file, root)
            if app_file.suffix.casefold() == ".css":
                errors.add(f"{displayed_path}: app-local CSS is forbidden")
            if app_file not in rust_source_files:
                continue
            try:
                source = app_file.read_text(encoding="utf-8", errors="replace")
            except OSError as error:
                errors.add(f"{displayed_path}: cannot inspect Rust source: {error}")
                continue
            source_errors, included_sources = rust_compile_time_source_violations(
                source, app_file, package_root, root
            )
            errors.update(source_errors)
            rust_source_files.update(included_sources)
            pending_files.update(included_sources - scanned_files)
            code = rust_code_without_comments_or_strings(source)
            for match in RUST_UI_REFERENCE.finditer(code):
                if not is_complete_rust_identifier_reference(match, code):
                    continue
                referenced_name = match.group("import") or match.group("path")
                errors.add(
                    f"{displayed_path}: raw Rust UI reference to {referenced_name!r} "
                    "is forbidden"
                )

    return sorted(errors)


def run_cargo_metadata(root: Path) -> dict[str, Any]:
    command = ["cargo", "metadata", "--locked", "--format-version", "1"]
    result = subprocess.run(
        command,
        cwd=root,
        capture_output=True,
        check=False,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or f"cargo metadata exited {result.returncode}"
        raise RuntimeError(detail)
    try:
        metadata = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"cargo metadata returned invalid JSON: {error}") from error
    if not isinstance(metadata, dict):
        raise RuntimeError("cargo metadata did not return a JSON object")
    return metadata


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    try:
        metadata = run_cargo_metadata(root)
    except (OSError, RuntimeError) as error:
        print(
            f"Architecture check could not read Cargo metadata: {error}",
            file=sys.stderr,
        )
        return 2

    errors = check_architecture(metadata, root)
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        print(
            f"Architecture check failed with {len(errors)} violation(s).",
            file=sys.stderr,
        )
        return 1

    app_count = len(workspace_app_packages(metadata, root))
    if app_count == 0:
        print(
            "Architecture check found 0 workspace app packages under apps/; "
            "app conformance not demonstrated."
        )
    else:
        print(f"Architecture check passed for {app_count} workspace app package(s).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
