from __future__ import annotations

import argparse
import hashlib
import json
import sys
import xml.etree.ElementTree as ET
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def unique_ids(values: list[str], label: str) -> None:
    require(isinstance(values, list) and all(isinstance(v, str) and v for v in values),
            f"{label}: expected nonempty string IDs")
    require(len(values) == len(set(values)), f"{label}: duplicate IDs")


def validate_classic_source_mapping(root: Path) -> tuple[int, int]:
    spec = (root / "qa/spec/classic").resolve()

    def path(name: str) -> Path:
        require(isinstance(name, str) and bool(name) and Path(name).name == name,
                "specification references must be filenames")
        result = (spec / name).resolve()
        require(result.is_relative_to(spec), f"{name}: path escapes specification")
        return result

    def load(name: str) -> dict[str, Any]:
        value = json.loads(path(name).read_text(), object_pairs_hook=unique_object,
                           parse_float=Decimal, parse_constant=invalid_constant)
        require(isinstance(value, dict), f"{name}: expected JSON object")
        return value

    def invalid_constant(value: str) -> None:
        raise ValueError(f"invalid JSON numeric constant: {value}")

    manifest = load("manifest.json")
    files = manifest["componentFiles"]
    unique_ids(files, "componentFiles")
    require({"buttons.json", "menus.json", "text-fields.json"} <= set(files),
            "button/menu/text-field files missing")
    documents = {name: load(name) for name in files}
    catalog = load(manifest["sourceNodeCatalogFile"])
    inventory = load(manifest["inventoryFile"])
    data = path(manifest["sourceMetadataFile"]).read_bytes()
    require(b"<!DOCTYPE" not in data and b"<!ENTITY" not in data,
            "XML document types/entities are not supported")
    xml = ET.fromstring(data)
    digest = hashlib.sha256(data).hexdigest()
    require(digest == manifest["source"]["historicalMetadata"]["sha256"],
            "XML SHA-256 differs from manifest")
    require(manifest["status"] == "DRAFT_SOURCE_METADATA_INCOMPLETE",
            "cached manifest cannot claim completion")
    for key, value in {"status": "HISTORICAL_CACHED_XML_REVISION_UNKNOWN",
                       "sourceRevision": None, "retrievedUtc": None}.items():
        require(manifest["source"]["historicalMetadata"][key] == value,
                f"manifest historical {key} overstates cached authority")
    require(manifest["source"]["sourceRevision"] is None
            and manifest["source"]["sourceLastModified"] is None
            and manifest["source"]["scale"]["sourceScale"] is None
            and manifest["source"]["assetLicense"]["status"] == "UNKNOWN"
            and manifest["source"]["typography"]["redistributionLicense"] == "UNKNOWN",
            "manifest revision/scale/licenses must remain unknown")
    for field in ("fontFamily", "fontVersion", "metricsSource"):
        require(manifest["source"]["typography"][field] is None,
                f"manifest typography {field} must remain unknown")
    authority = {
        "metadataStatus": "HISTORICAL_CACHED_XML_REVISION_UNKNOWN",
        "geometryStatus": "CACHED_SOURCE_NODE_BOUNDS_ONLY",
        "sourceRevision": None, "sourceScale": None, "retrievedUtc": None,
        "fontStatus": "UNKNOWN", "licenseStatus": "UNKNOWN", "currentConformance": "NOT_RUN",
    }
    for name, document, status in (
        ("catalog", catalog, "HISTORICAL_METADATA_ONLY_NOT_CONFORMANCE"),
        ("inventory", inventory, "DRAFT_SOURCE_MAPPING_PENDING"),
        ("buttons", documents["buttons.json"], "PARTIAL_CACHED_SOURCE_MAPPING_NOT_CONFORMANCE"),
        ("menus", documents["menus.json"], "PARTIAL_CACHED_SOURCE_MAPPING_NOT_CONFORMANCE"),
        ("text-fields", documents["text-fields.json"], "PARTIAL_CACHED_SOURCE_MAPPING_NOT_CONFORMANCE"),
    ):
        require(document["status"] == status, f"{name}: invalid cached document status")
        source = document["source"]
        for key, value in authority.items():
            require(source[key] == value, f"{name}: {key} overstates cached authority")
        require(source["metadataSha256"] == digest, "XML SHA-256 differs from source")
        for key in ("fileKey", "rootPageNodeId"):
            require(source[key] == manifest["source"][key], f"source {key} differs")
        require(source["metadataFile"] == manifest["sourceMetadataFile"]
                and source["nodeCatalogFile"] == manifest["sourceNodeCatalogFile"],
                "source artifact filenames differ from manifest")
    require(xml.attrib["id"] == manifest["source"]["rootPageNodeId"], "XML root differs")
    parents = {child.attrib["id"]: node.attrib["id"] for node in xml.iter() for child in node}
    expected: dict[str, dict[str, Any]] = {}
    for node in xml.iter():
        node_id = node.attrib["id"]
        require(bool(node_id) and node_id not in expected, f"duplicate/empty XML ID: {node_id}")
        geometry = {key: Decimal(node.attrib[key]) for key in ("x", "y", "width", "height")}
        require(all(value.is_finite() for value in geometry.values()), f"{node_id}: nonfinite bounds")
        expected[node_id] = {
            "nodeId": node_id, "name": node.attrib["name"], "type": node.tag,
            "parentNodeId": parents.get(node_id), "geometry": geometry,
            "childNodeIds": [child.attrib["id"] for child in node],
            "metadataAttributes": {k: v for k, v in node.attrib.items()
                                   if k not in {"id", "name", "x", "y", "width", "height"}},
        }

    def records(entries: list[dict[str, Any]], label: str) -> list[str]:
        require(isinstance(entries, list), f"{label}: expected record array")
        ids = [entry["nodeId"] for entry in entries]
        unique_ids(ids, label)
        for entry in entries:
            node_id = entry["nodeId"]
            require(node_id in expected, f"{label}: missing XML node {node_id}")
            geometry = entry["geometry"]
            require(all(type(v) in (int, Decimal) for v in geometry.values()),
                    f"{node_id}: bounds must be JSON numbers")
            for key, value in expected[node_id].items():
                actual = entry.get(key, {}) if key == "metadataAttributes" else entry[key]
                require(actual == value, f"{label}: {node_id} {key} differs from XML")
            if "sourceVariantProperties" in entry:
                properties = dict(part.split("=", 1) for part in entry["name"].split(", ") if "=" in part)
                require(entry["sourceVariantProperties"] == properties,
                        f"{node_id}: variant properties differ from source name")
        return ids

    require(set(records(catalog["nodes"], "catalog")) == set(expected), "catalog node set differs")
    components = inventory["components"]
    unique_ids([item["id"] for item in components], "inventory components")
    indexed = {item["id"]: item for item in components}
    mapped = 0
    for item in components:
        require(item["specificationStatus"] == "NOT_EXTRACTED", "cached mapping cannot claim extraction")
        candidates = item.get("candidateSourceNodeIds", [])
        unique_ids(candidates, item["id"] + " candidates")
        require(all(node_id in expected for node_id in candidates), f"{item['id']}: candidate XML node missing")
        status = item["sourceMappingStatus"]
        require(status in {"UNKNOWN", "MAPPED"}, f"{item['id']}: invalid mapping status")
        if status == "UNKNOWN":
            require(item["sourceNodeId"] is None, f"{item['id']}: unknown mapping has a source ID")
            for key in ("sourceNodeIds", "sourceParentNodeId", "widthPx", "heightPx"):
                require(item.get(key) is None, f"{item['id']}: unknown mapping claims {key}")
            continue
        mapped += 1
        require(item["sourceMetadataStatus"] == authority["metadataStatus"]
                and item["geometryStatus"] == authority["geometryStatus"],
                f"{item['id']}: mapped authority must remain historical")
        ids = item["sourceNodeIds"]
        unique_ids(ids, item["id"])
        primary = item["sourceNodeId"]
        require(primary in ids and all(node_id in expected for node_id in ids),
                f"{item['id']}: representative/variant XML node missing")
        require(all(expected[node_id]["parentNodeId"] == item["sourceParentNodeId"] for node_id in ids),
                f"{item['id']}: variant parent differs from XML")
        for dimension in ("width", "height"):
            value = item[dimension + "Px"]
            require(type(value) in (int, Decimal) and value == expected[primary]["geometry"][dimension],
                    f"{item['id']}: mapped {dimension} differs from XML")
    for name in ("buttons.json", "menus.json"):
        document = documents[name]
        require(document["sourceContainerNodeId"] in expected,
                f"{name}: source container missing from XML")
        variants = records(document["sourceVariants"], name)
        require(document["sourceNodeIds"] == variants, f"{name}: variant ID list differs")
        if name == "buttons.json":
            group = expected[document["sourceContainerNodeId"]]
            require(group["name"] == "Button" and group["type"] == "frame",
                    "button source group differs")
            required_variants = set(group["childNodeIds"])
        else:
            group = expected[document["sourceContainerNodeId"]]
            require(group["name"] == "Menus" and group["type"] == "frame", "menu source container differs")
            bases = [node_id for node_id in group["childNodeIds"]
                     if expected[node_id]["name"].startswith("Base/")
                     or expected[node_id]["name"] == "Menu bar"]
            required_variants = set(bases)
            for node_id in bases:
                required_variants.update(expected[node_id]["childNodeIds"])
        require(set(variants) == required_variants, f"{name}: XML variant coverage differs")
        compositions = records(document.get("sourceCompositions", []), name + " compositions")
        covered = set(variants + compositions)
        for item in components:
            if item["sourceMappingStatus"] == "MAPPED" and item["id"].startswith(
                "button." if name == "buttons.json" else "menu."
            ):
                require(set(item["sourceNodeIds"]) <= covered,
                        f"{item['id']}: mapped variants absent from {name}")
    for name in ("buttons.json", "menus.json", "text-fields.json"):
        document = documents[name]
        unique_ids(document["sourceNodeIds"], name + " source IDs")
        require(all(node_id in expected for node_id in document["sourceNodeIds"]),
                f"{name}: source XML node missing")
        unique_ids([item["id"] for item in document["items"]], name + " items")
        for item in document["items"]:
            require(item["id"] in indexed, f"{name}: item absent from inventory")
            for key in ("sourceNodeId", "sourceNodeIds", "sourceParentNodeId", "widthPx", "heightPx"):
                require(item.get(key) == indexed[item["id"]].get(key), f"{name}: {item['id']} {key} differs")
            if item["sourceNodeId"] is not None:
                require(item["sourceMappingStatus"] == "MAPPED"
                        and set(item["sourceNodeIds"]) <= set(document["sourceNodeIds"])
                        and item["sourceMetadataStatus"] == authority["metadataStatus"]
                        and item["geometryStatus"] == authority["geometryStatus"],
                        f"{name}: {item['id']} overstates cached authority")
    return len(expected), mapped


def main() -> int:
    parser = argparse.ArgumentParser(description="Check cached Classic source integrity, not conformance")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1],
                        help="repository root containing qa/ and scripts/")
    try:
        nodes, mapped = validate_classic_source_mapping(parser.parse_args().root)
    except (OSError, ValueError, TypeError, KeyError, AttributeError, InvalidOperation, ET.ParseError) as error:
        print(f"ERROR: Classic source mapping: {error}", file=sys.stderr)
        return 1
    print(f"Classic cached source integrity passed: {nodes} nodes, {mapped} mappings; conformance NOT_RUN.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
