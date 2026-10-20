from __future__ import annotations

import importlib.util
import json
import shutil
from pathlib import Path
from typing import Callable, cast

import pytest


CHECKER_PATH = Path(__file__).with_name("check-classic-source-mapping.py")
CHECKER_SPEC = importlib.util.spec_from_file_location(
    "classic_source_mapping", CHECKER_PATH
)
if CHECKER_SPEC is None or CHECKER_SPEC.loader is None:
    raise ImportError(f"Cannot load Classic source checker from {CHECKER_PATH}")
CHECKER_MODULE = importlib.util.module_from_spec(CHECKER_SPEC)
CHECKER_SPEC.loader.exec_module(CHECKER_MODULE)
validate_mapping = cast(
    Callable[[Path], tuple[int, int]], CHECKER_MODULE.validate_classic_source_mapping
)


def copy_spec(root: Path) -> None:
    shutil.copytree(
        CHECKER_PATH.parent.parent / "qa/spec/classic", root / "qa/spec/classic"
    )


def test_current_cached_source_mapping(tmp_path: Path) -> None:
    copy_spec(tmp_path)

    nodes, mappings = validate_mapping(tmp_path)

    assert nodes > 0 and mappings > 0


@pytest.mark.parametrize(
    ("mutation", "filename", "reason"),
    [
        ("variant", "buttons.json", "XML variant coverage differs"),
        ("authority", "buttons.json", "currentConformance overstates cached authority"),
        ("textfield", "text-fields.json", r"text_field\.text widthPx differs"),
        ("unknown", "inventory.json", r"foundation\.geometry: unknown mapping claims widthPx"),
        ("catalog", "source-nodes.json", "name differs from XML"),
        ("hash", "source-metadata.xml", "XML SHA-256 differs from manifest"),
    ],
)
def test_rejects_corrupted_source_mapping(
    tmp_path: Path, mutation: str, filename: str, reason: str
) -> None:
    copy_spec(tmp_path)
    path = tmp_path / "qa/spec/classic" / filename
    if mutation == "hash":
        path.write_bytes(path.read_bytes() + b"\n")
    else:
        document = json.loads(path.read_text(encoding="utf-8"))
        if mutation == "variant":
            document["sourceNodeIds"].remove("73:3674")
            document["sourceVariants"] = [
                record for record in document["sourceVariants"]
                if record["nodeId"] != "73:3674"
            ]
        elif mutation == "authority":
            document["source"]["currentConformance"] = "PASS"
        elif mutation == "textfield":
            item = next(
                item for item in document["items"] if item["id"] == "text_field.text"
            )
            item["widthPx"] = 171
        elif mutation == "unknown":
            item = next(
                item for item in document["components"]
                if item["id"] == "foundation.geometry"
            )
            item["widthPx"] = 42
        elif mutation == "catalog":
            document["nodes"][0]["name"] = "Incorrect source name"
        path.write_text(json.dumps(document), encoding="utf-8")

    with pytest.raises(ValueError, match=reason):
        validate_mapping(tmp_path)
