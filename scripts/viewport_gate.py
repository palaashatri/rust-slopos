#!/usr/bin/env python3
"""Validate compositor-owned viewport state against an exact PNG framebuffer.

The validator deliberately consumes an explicit schema instead of inferring
geometry from screenshots or from host-window dimensions.  A normal run is
valid only for a runtime capture whose producer identifies itself as the
compositor.  ``--self-test`` creates a deterministic fixture under
``target/viewport-gate-self-test`` and proves both an accepted image and a
three-pixel clear edge rejection.
"""

from __future__ import annotations

import argparse
import binascii
import hashlib
import json
import os
import re
import struct
import sys
import zlib
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Mapping, Sequence


SCHEMA_VERSION = 1
MAX_FRAMEBUFFER_BYTES = 512 * 1024 * 1024
MAX_FRAMEBUFFER_PIXELS = 64 * 1024 * 1024
COMMIT_RE = re.compile(r"^[0-9a-fA-F]{40}$")
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


class GateError(ValueError):
    """A state or framebuffer contract violation."""


def utc_now() -> str:
    """Return an ISO-8601 UTC timestamp for evidence."""

    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace(
        "+00:00", "Z"
    )


def json_object(value: Any, path: str) -> Mapping[str, Any]:
    """Require a JSON object and include its path in the error."""

    if not isinstance(value, Mapping):
        raise GateError(f"{path} must be an object")
    return value


def required(mapping: Mapping[str, Any], key: str, path: str) -> Any:
    """Read a required object member."""

    if key not in mapping:
        raise GateError(f"{path}.{key} is required")
    return mapping[key]


def string_value(value: Any, path: str, nonempty: bool = True) -> str:
    """Require a JSON string."""

    if not isinstance(value, str) or (nonempty and not value):
        raise GateError(f"{path} must be a non-empty string")
    return value


def integer_value(value: Any, path: str, minimum: int | None = None) -> int:
    """Require a JSON integer, excluding booleans."""

    if isinstance(value, bool) or not isinstance(value, int):
        raise GateError(f"{path} must be an integer")
    if minimum is not None and value < minimum:
        raise GateError(f"{path} must be >= {minimum}")
    return value


def bool_value(value: Any, path: str) -> bool:
    """Require a JSON boolean."""

    if not isinstance(value, bool):
        raise GateError(f"{path} must be a boolean")
    return value


def dimension_object(value: Any, path: str, allow_zero: bool = False) -> dict[str, int]:
    """Read a width/height object with positive or explicitly zero values."""

    obj = json_object(value, path)
    minimum = 0 if allow_zero else 1
    return {
        "width": integer_value(required(obj, "width", path), f"{path}.width", minimum),
        "height": integer_value(
            required(obj, "height", path), f"{path}.height", minimum
        ),
    }


def rect_object(value: Any, path: str) -> dict[str, int]:
    """Read an integer rectangle."""

    obj = json_object(value, path)
    return {
        "x": integer_value(required(obj, "x", path), f"{path}.x"),
        "y": integer_value(required(obj, "y", path), f"{path}.y"),
        "width": integer_value(required(obj, "width", path), f"{path}.width", 1),
        "height": integer_value(
            required(obj, "height", path), f"{path}.height", 1
        ),
    }


def rational_object(value: Any, path: str) -> dict[str, int]:
    """Read a positive rational without accepting lossy floating-point input."""

    obj = json_object(value, path)
    numerator = integer_value(
        required(obj, "numerator", path), f"{path}.numerator", 1
    )
    denominator = integer_value(
        required(obj, "denominator", path), f"{path}.denominator", 1
    )
    return {"numerator": numerator, "denominator": denominator}


def rational_dimension(logical: int, scale: Mapping[str, int]) -> int:
    """Apply the compositor's ceil(logical * numerator / denominator) rule."""

    numerator = scale["numerator"]
    denominator = scale["denominator"]
    return (logical * numerator + denominator - 1) // denominator


def sha256_file(path: Path) -> str:
    """Hash a file in bounded chunks."""

    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while True:
            block = stream.read(1024 * 1024)
            if not block:
                break
            digest.update(block)
    return digest.hexdigest()


def sha256_bytes(data: bytes) -> str:
    """Hash bytes as lowercase hexadecimal SHA-256."""

    return hashlib.sha256(data).hexdigest()


def load_json(path: Path) -> tuple[Mapping[str, Any], bytes]:
    """Load a UTF-8 JSON object and retain its exact bytes for evidence."""

    try:
        raw = path.read_bytes()
    except OSError as error:
        raise GateError(f"cannot read state {path}: {error}") from error
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise GateError(f"state is not valid UTF-8 JSON: {error}") from error
    return json_object(value, "state"), raw


def decode_png(path: Path) -> tuple[int, int, bytes]:
    """Decode an un-interlaced 8-bit RGB/RGBA PNG into RGBA bytes.

    This small decoder keeps the gate dependency-free on a QA VM.  It verifies
    PNG chunk CRCs and all scanline filters needed by normal compositor output.
    """

    try:
        size = path.stat().st_size
    except OSError as error:
        raise GateError(f"cannot stat framebuffer {path}: {error}") from error
    if size <= len(PNG_SIGNATURE) or size > MAX_FRAMEBUFFER_BYTES:
        raise GateError(f"framebuffer size is outside bounds: {size}")
    try:
        data = path.read_bytes()
    except OSError as error:
        raise GateError(f"cannot read framebuffer {path}: {error}") from error
    if data[:8] != PNG_SIGNATURE:
        raise GateError("framebuffer is not a PNG")

    width = height = bit_depth = color_type = interlace = None
    idat = bytearray()
    offset = len(PNG_SIGNATURE)
    saw_iend = False
    while offset + 12 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_end = offset + 12 + length
        if chunk_end > len(data):
            raise GateError("truncated PNG chunk")
        kind = data[offset + 4 : offset + 8]
        payload = data[offset + 8 : offset + 8 + length]
        expected_crc = struct.unpack(">I", data[offset + 8 + length : chunk_end])[0]
        actual_crc = binascii.crc32(kind + payload) & 0xFFFFFFFF
        if actual_crc != expected_crc:
            raise GateError(f"PNG CRC mismatch in {kind.decode('latin1')}")
        if kind == b"IHDR":
            if length != 13 or width is not None:
                raise GateError("PNG must contain exactly one valid IHDR")
            width, height, bit_depth, color_type, compression, filter_method, interlace = (
                struct.unpack(">IIBBBBB", payload)
            )
            if compression != 0 or filter_method != 0:
                raise GateError("unsupported PNG compression or filter method")
            if width <= 0 or height <= 0:
                raise GateError("PNG dimensions must be positive")
            if width * height > MAX_FRAMEBUFFER_PIXELS:
                raise GateError("PNG pixel count exceeds gate limit")
            if bit_depth != 8 or color_type not in (2, 6) or interlace != 0:
                raise GateError("PNG must be non-interlaced 8-bit RGB or RGBA")
        elif kind == b"IDAT":
            idat.extend(payload)
        elif kind == b"IEND":
            if length != 0:
                raise GateError("PNG IEND payload must be empty")
            saw_iend = True
            break
        offset = chunk_end

    if width is None or height is None or not idat or not saw_iend:
        raise GateError("PNG is missing IHDR, IDAT, or IEND")
    channels = 4 if color_type == 6 else 3
    stride = width * channels
    try:
        decoded = zlib.decompress(bytes(idat))
    except zlib.error as error:
        raise GateError(f"PNG IDAT decompression failed: {error}") from error
    expected_size = (stride + 1) * height
    if len(decoded) != expected_size:
        raise GateError(
            f"PNG scanline length mismatch: expected {expected_size}, got {len(decoded)}"
        )

    rows: list[bytes] = []
    previous = bytearray(stride)
    cursor = 0
    for row_index in range(height):
        filter_type = decoded[cursor]
        source = decoded[cursor + 1 : cursor + 1 + stride]
        cursor += stride + 1
        row = bytearray(stride)
        for index, value in enumerate(source):
            left = row[index - channels] if index >= channels else 0
            up = previous[index]
            up_left = previous[index - channels] if index >= channels else 0
            if filter_type == 0:
                predictor = 0
            elif filter_type == 1:
                predictor = left
            elif filter_type == 2:
                predictor = up
            elif filter_type == 3:
                predictor = (left + up) // 2
            elif filter_type == 4:
                estimate = left + up - up_left
                distance_left = abs(estimate - left)
                distance_up = abs(estimate - up)
                distance_up_left = abs(estimate - up_left)
                if distance_left <= distance_up and distance_left <= distance_up_left:
                    predictor = left
                elif distance_up <= distance_up_left:
                    predictor = up
                else:
                    predictor = up_left
            else:
                raise GateError(f"unsupported PNG scanline filter {filter_type}")
            row[index] = (value + predictor) & 0xFF
        rows.append(bytes(row))
        previous = row

    if color_type == 6:
        rgba = b"".join(rows)
    else:
        rgba_buffer = bytearray(width * height * 4)
        destination = 0
        for row in rows:
            for index in range(0, len(row), 3):
                rgba_buffer[destination : destination + 3] = row[index : index + 3]
                rgba_buffer[destination + 3] = 255
                destination += 4
        rgba = bytes(rgba_buffer)
    return width, height, rgba


def color_value(value: Any, path: str) -> list[int]:
    """Read an RGBA color expressed as four 8-bit integers."""

    if not isinstance(value, list) or len(value) != 4:
        raise GateError(f"{path} must contain four RGBA integers")
    result: list[int] = []
    for index, component in enumerate(value):
        result.append(integer_value(component, f"{path}[{index}]", 0))
        if result[-1] > 255:
            raise GateError(f"{path}[{index}] must be <= 255")
    return result


def edge_band_depths(
    width: int,
    height: int,
    pixels: bytes,
    clear_color: Sequence[int],
    tolerance: int,
) -> dict[str, int]:
    """Measure complete clear-color rows/columns contiguous at each edge."""

    def is_clear(x: int, y: int) -> bool:
        start = (y * width + x) * 4
        return all(
            abs(pixels[start + channel] - clear_color[channel]) <= tolerance
            for channel in range(4)
        )

    def line_clear(edge: str, depth: int) -> bool:
        if edge == "top":
            return all(is_clear(x, depth) for x in range(width))
        if edge == "bottom":
            return all(is_clear(x, height - 1 - depth) for x in range(width))
        if edge == "left":
            return all(is_clear(depth, y) for y in range(height))
        return all(is_clear(width - 1 - depth, y) for y in range(height))

    result: dict[str, int] = {}
    for edge, limit in (
        ("top", height),
        ("bottom", height),
        ("left", width),
        ("right", width),
    ):
        depth = 0
        while depth < limit and line_clear(edge, depth):
            depth += 1
        result[edge] = depth
    return result


def resolve_declared_framebuffer(
    state_path: Path, frame_path: Path, declared: Any
) -> Path:
    """Require the CLI framebuffer path to match the state declaration."""

    declared_string = string_value(declared, "state.framebuffer.path")
    declared_path = Path(declared_string)
    if not declared_path.is_absolute():
        declared_path = state_path.parent / declared_path
    try:
        expected = declared_path.resolve()
        actual = frame_path.resolve()
    except OSError as error:
        raise GateError(f"cannot resolve framebuffer path: {error}") from error
    if expected != actual:
        raise GateError(
            f"CLI framebuffer {actual} does not match state framebuffer {expected}"
        )
    return actual


def evidence_base(state_path: Path, frame_path: Path, timestamp: str) -> dict[str, Any]:
    """Create an evidence object safe to write even for malformed input."""

    return {
        "schema_version": SCHEMA_VERSION,
        "component": "slopos-viewport-gate",
        "status": "failed",
        "evidence_level": "unverified",
        "started_at_utc": timestamp,
        "state_path": str(state_path),
        "framebuffer_path": str(frame_path),
        "commit": None,
        "branch": None,
        "backend": None,
        "output": None,
        "framebuffer": None,
        "layers": [],
        "checks": {},
        "failures": [],
    }


def evaluate(
    state_path: Path,
    frame_path: Path,
    allow_fixture: bool = False,
    timestamp: str | None = None,
) -> dict[str, Any]:
    """Evaluate one state/framebuffer pair and return machine-readable evidence."""

    evidence = evidence_base(state_path, frame_path, timestamp or utc_now())
    try:
        state, raw_state = load_json(state_path)
        evidence["state_sha256"] = sha256_bytes(raw_state)

        schema = integer_value(
            required(state, "schema_version", "state"), "state.schema_version", 1
        )
        if schema != SCHEMA_VERSION:
            raise GateError(
                f"unsupported state.schema_version {schema}; expected {SCHEMA_VERSION}"
            )
        evidence["checks"]["schema_version"] = True

        commit = string_value(required(state, "commit", "state"), "state.commit")
        if not COMMIT_RE.fullmatch(commit):
            raise GateError("state.commit must be a 40-character hexadecimal SHA")
        branch = string_value(required(state, "branch", "state"), "state.branch")
        backend = string_value(required(state, "backend", "state"), "state.backend")
        if not allow_fixture and backend.lower() == "headless":
            raise GateError(
                "runtime viewport gate requires a framebuffer backend; headless output is not capture evidence"
            )
        evidence["commit"] = commit
        evidence["branch"] = branch
        evidence["backend"] = backend

        provenance = json_object(
            required(state, "provenance", "state"), "state.provenance"
        )
        provenance_kind = string_value(
            required(provenance, "kind", "state.provenance"),
            "state.provenance.kind",
        )
        capture_kind = string_value(
            required(provenance, "capture", "state.provenance"),
            "state.provenance.capture",
        )
        if allow_fixture:
            if provenance_kind != "fixture" or capture_kind != "fixture_framebuffer":
                raise GateError("self-test state must identify a fixture framebuffer")
        elif provenance_kind != "runtime" or capture_kind != "compositor_framebuffer":
            raise GateError(
                "runtime gate requires provenance kind=runtime and "
                "capture=compositor_framebuffer"
            )
        evidence["evidence_level"] = (
            "self_test_fixture" if allow_fixture else "runtime_viewport"
        )
        evidence["checks"]["provenance"] = True

        coordinate_space = string_value(
            required(state, "coordinate_space", "state"), "state.coordinate_space"
        )
        if coordinate_space != "logical":
            raise GateError("state.coordinate_space must be logical")

        output = json_object(required(state, "output", "state"), "state.output")
        output_name = string_value(
            required(output, "name", "state.output"), "state.output.name"
        )
        logical = dimension_object(
            required(output, "logical", "state.output"), "state.output.logical"
        )
        physical = dimension_object(
            required(output, "physical", "state.output"), "state.output.physical"
        )
        requested_scale = rational_object(
            required(output, "requested_scale", "state.output"),
            "state.output.requested_scale",
        )
        effective_scale = rational_object(
            required(output, "effective_scale", "state.output"),
            "state.output.effective_scale",
        )
        output_revision = integer_value(
            required(output, "revision", "state.output"), "state.output.revision", 1
        )
        frame_revision = integer_value(
            required(output, "frame_revision", "state.output"),
            "state.output.frame_revision",
            1,
        )
        evidence["output"] = {
            "name": output_name,
            "logical": logical,
            "physical": physical,
            "requested_scale": requested_scale,
            "effective_scale": effective_scale,
            "revision": output_revision,
            "frame_revision": frame_revision,
        }
        expected_physical = {
            "width": rational_dimension(logical["width"], effective_scale),
            "height": rational_dimension(logical["height"], effective_scale),
        }
        if physical != expected_physical:
            raise GateError(
                "physical dimensions do not equal ceil(logical * effective_scale): "
                f"expected {expected_physical}, got {physical}"
            )
        evidence["checks"]["logical_physical_effective_scale"] = True

        framebuffer = json_object(
            required(state, "framebuffer", "state"), "state.framebuffer"
        )
        actual_frame = resolve_declared_framebuffer(
            state_path,
            frame_path,
            required(framebuffer, "path", "state.framebuffer"),
        )
        image_format = string_value(
            required(framebuffer, "format", "state.framebuffer"),
            "state.framebuffer.format",
        ).lower()
        if image_format != "png":
            raise GateError("state.framebuffer.format must be png")
        declared_dimensions = dimension_object(
            required(framebuffer, "dimensions", "state.framebuffer"),
            "state.framebuffer.dimensions",
        )
        clear_color = color_value(
            required(framebuffer, "clear_color", "state.framebuffer"),
            "state.framebuffer.clear_color",
        )
        tolerance = integer_value(
            framebuffer.get("clear_tolerance", 0),
            "state.framebuffer.clear_tolerance",
            0,
        )
        if tolerance > 32:
            raise GateError("state.framebuffer.clear_tolerance must be <= 32")
        declared_hash = string_value(
            required(framebuffer, "sha256", "state.framebuffer"),
            "state.framebuffer.sha256",
        ).lower()
        if not re.fullmatch(r"[0-9a-f]{64}", declared_hash):
            raise GateError("state.framebuffer.sha256 must be 64 lowercase hex characters")
        actual_hash = sha256_file(actual_frame)
        if actual_hash != declared_hash:
            raise GateError(
                f"framebuffer SHA-256 mismatch: state={declared_hash} actual={actual_hash}"
            )
        image_width, image_height, pixels = decode_png(actual_frame)
        actual_dimensions = {"width": image_width, "height": image_height}
        if declared_dimensions != actual_dimensions:
            raise GateError(
                f"declared framebuffer dimensions {declared_dimensions} do not match "
                f"PNG dimensions {actual_dimensions}"
            )
        if actual_dimensions != physical:
            raise GateError(
                f"PNG dimensions {actual_dimensions} do not exactly match physical "
                f"output {physical}"
            )
        evidence["framebuffer"] = {
            "path": str(actual_frame),
            "format": image_format,
            "declared_dimensions": declared_dimensions,
            "actual_dimensions": actual_dimensions,
            "sha256": actual_hash,
            "clear_color": clear_color,
            "clear_tolerance": tolerance,
        }
        evidence["checks"]["framebuffer_exact_dimensions"] = True
        evidence["checks"]["framebuffer_sha256"] = True

        layers_value = required(state, "layers", "state")
        if not isinstance(layers_value, list) or not layers_value:
            raise GateError("state.layers must be a non-empty array")
        parsed_layers: list[dict[str, Any]] = []
        roles: dict[str, dict[str, int]] = {}
        for index, raw_layer in enumerate(layers_value):
            layer_path = f"state.layers[{index}]"
            layer = json_object(raw_layer, layer_path)
            namespace = string_value(
                required(layer, "namespace", layer_path), f"{layer_path}.namespace"
            )
            protocol_layer = string_value(
                required(layer, "layer", layer_path), f"{layer_path}.layer"
            )
            role = string_value(
                required(layer, "role", layer_path), f"{layer_path}.role"
            )
            layer_output = string_value(
                required(layer, "output", layer_path), f"{layer_path}.output"
            )
            if layer_output != output_name:
                raise GateError(
                    f"{layer_path}.output {layer_output!r} does not match output {output_name!r}"
                )
            geometry_space = string_value(
                required(layer, "geometry_space", layer_path),
                f"{layer_path}.geometry_space",
            )
            if geometry_space != "logical":
                raise GateError(f"{layer_path}.geometry_space must be logical")
            requested = dimension_object(
                required(layer, "requested", layer_path),
                f"{layer_path}.requested",
                allow_zero=True,
            )
            configured = dimension_object(
                required(layer, "configured", layer_path),
                f"{layer_path}.configured",
            )
            geometry = rect_object(
                required(layer, "geometry", layer_path), f"{layer_path}.geometry"
            )
            if configured != {
                "width": geometry["width"],
                "height": geometry["height"],
            }:
                raise GateError(
                    f"{layer_path}.configured dimensions do not match geometry"
                )
            if geometry["x"] < 0 or geometry["y"] < 0:
                raise GateError(f"{layer_path}.geometry must be local to the output")
            if geometry["x"] + geometry["width"] > logical["width"] or geometry[
                "y"
            ] + geometry["height"] > logical["height"]:
                raise GateError(f"{layer_path}.geometry lies outside logical output")
            active = bool_value(
                required(layer, "active", layer_path), f"{layer_path}.active"
            )
            if not active:
                raise GateError(f"{layer_path} is not active in the captured frame")
            configure_serial = integer_value(
                required(layer, "configure_serial", layer_path),
                f"{layer_path}.configure_serial",
                1,
            )
            acknowledged = bool_value(
                required(layer, "acknowledged", layer_path),
                f"{layer_path}.acknowledged",
            )
            ack_serial = integer_value(
                required(layer, "ack_serial", layer_path),
                f"{layer_path}.ack_serial",
                1,
            )
            if not acknowledged or ack_serial != configure_serial:
                raise GateError(
                    f"{layer_path} configure serial was not acknowledged: "
                    f"configure={configure_serial} ack={ack_serial}"
                )
            committed = bool_value(
                required(layer, "committed", layer_path), f"{layer_path}.committed"
            )
            committed_frame_revision = integer_value(
                required(layer, "committed_frame_revision", layer_path),
                f"{layer_path}.committed_frame_revision",
                1,
            )
            if not committed or committed_frame_revision != frame_revision:
                raise GateError(
                    f"{layer_path} was not committed for frame revision {frame_revision}"
                )
            parsed = {
                "namespace": namespace,
                "layer": protocol_layer,
                "role": role,
                "output": layer_output,
                "geometry_space": geometry_space,
                "requested": requested,
                "configured": configured,
                "geometry": geometry,
                "active": active,
                "configure_serial": configure_serial,
                "acknowledged": acknowledged,
                "ack_serial": ack_serial,
                "committed": committed,
                "committed_frame_revision": committed_frame_revision,
            }
            if role in roles:
                raise GateError(f"duplicate layer role {role!r}")
            roles[role] = geometry
            parsed_layers.append(parsed)

        for required_role in ("background", "menu", "dock"):
            if required_role not in roles:
                raise GateError(f"missing required {required_role} layer")
        background = roles["background"]
        if background != {
            "x": 0,
            "y": 0,
            "width": logical["width"],
            "height": logical["height"],
        }:
            raise GateError("background geometry does not cover the full logical output")
        menu = roles["menu"]
        if menu["x"] != 0 or menu["y"] != 0 or menu["width"] != logical["width"]:
            raise GateError("menu geometry must be a full-width top strip")
        dock = roles["dock"]
        if (
            dock["x"] != 0
            or dock["y"] + dock["height"] != logical["height"]
            or dock["width"] != logical["width"]
        ):
            raise GateError("Dock geometry must be a full-width bottom strip")
        evidence["layers"] = parsed_layers
        evidence["checks"]["all_layers_configured_acknowledged_committed"] = True
        evidence["checks"]["background_full_output"] = True
        evidence["checks"]["menu_full_width"] = True
        evidence["checks"]["dock_full_width"] = True

        edge_bands = edge_band_depths(
            image_width, image_height, pixels, clear_color, tolerance
        )
        evidence["framebuffer"]["clear_edge_bands"] = edge_bands
        bad_edges = {edge: depth for edge, depth in edge_bands.items() if depth > 2}
        if bad_edges:
            raise GateError(
                "clear/unpainted edge band exceeds 2 pixels: "
                + ", ".join(f"{edge}={depth}" for edge, depth in sorted(bad_edges.items()))
            )
        evidence["checks"]["clear_edge_bands_at_most_two_pixels"] = True
        evidence["status"] = "passed"
    except GateError as error:
        evidence["failures"] = [str(error)]
    return evidence


def write_json(path: Path, value: Mapping[str, Any]) -> None:
    """Atomically write sorted, machine-readable evidence."""

    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    encoded = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
    temporary.write_bytes(encoded)
    os.replace(temporary, path)


def safe_slug(value: str) -> str:
    """Make a state value safe for a local evidence filename."""

    slug = re.sub(r"[^A-Za-z0-9_.-]+", "_", value)
    return slug[:120] or "unknown"


def default_artifact(state_path: Path) -> Path:
    """Choose an evidence path without requiring a shell-specific basename tool."""

    root = Path(os.environ.get("SLOPOS_QA_ARTIFACT_DIR", "artifacts/qa/viewport-runtime"))
    commit = "unknown-commit"
    output = "unknown-output"
    try:
        state = json.loads(state_path.read_text(encoding="utf-8"))
        if isinstance(state, Mapping):
            commit = str(state.get("commit", commit))
            output_value = state.get("output")
            if isinstance(output_value, Mapping):
                output = str(output_value.get("name", output))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        pass
    return root / f"{safe_slug(commit)}-{safe_slug(output)}.json"


def png_chunk(kind: bytes, payload: bytes) -> bytes:
    """Encode one deterministic PNG chunk."""

    crc = binascii.crc32(kind + payload) & 0xFFFFFFFF
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", crc)


def encode_rgba_png(width: int, height: int, pixels: bytes) -> bytes:
    """Encode deterministic filter-zero RGBA PNG bytes for self-test fixtures."""

    if len(pixels) != width * height * 4:
        raise ValueError("fixture pixel buffer has the wrong length")
    raw = bytearray()
    for row in range(height):
        raw.append(0)
        start = row * width * 4
        raw.extend(pixels[start : start + width * 4])
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"".join(
        (
            PNG_SIGNATURE,
            png_chunk(b"IHDR", ihdr),
            png_chunk(b"IDAT", zlib.compress(bytes(raw), level=9)),
            png_chunk(b"IEND", b""),
        )
    )


def fixture_state(frame_name: str, frame_hash: str) -> dict[str, Any]:
    """Build the deterministic self-test state contract."""

    output_name = "fixture-output"
    logical = {"width": 16, "height": 10}
    layers = [
        {
            "namespace": "slopos-background",
            "layer": "background",
            "role": "background",
            "output": output_name,
            "geometry_space": "logical",
            "requested": {"width": 0, "height": 0},
            "configured": logical,
            "geometry": {"x": 0, "y": 0, "width": 16, "height": 10},
            "active": True,
            "configure_serial": 11,
            "acknowledged": True,
            "ack_serial": 11,
            "committed": True,
            "committed_frame_revision": 7,
        },
        {
            "namespace": "slopos-i-menu",
            "layer": "top",
            "role": "menu",
            "output": output_name,
            "geometry_space": "logical",
            "requested": {"width": 0, "height": 0},
            "configured": {"width": 16, "height": 2},
            "geometry": {"x": 0, "y": 0, "width": 16, "height": 2},
            "active": True,
            "configure_serial": 12,
            "acknowledged": True,
            "ack_serial": 12,
            "committed": True,
            "committed_frame_revision": 7,
        },
        {
            "namespace": "slopos-i-dock",
            "layer": "bottom",
            "role": "dock",
            "output": output_name,
            "geometry_space": "logical",
            "requested": {"width": 0, "height": 0},
            "configured": {"width": 16, "height": 2},
            "geometry": {"x": 0, "y": 8, "width": 16, "height": 2},
            "active": True,
            "configure_serial": 13,
            "acknowledged": True,
            "ack_serial": 13,
            "committed": True,
            "committed_frame_revision": 7,
        },
    ]
    return {
        "schema_version": SCHEMA_VERSION,
        "commit": "0000000000000000000000000000000000000000",
        "branch": "viewport-gate-self-test",
        "backend": "fixture",
        "provenance": {"kind": "fixture", "capture": "fixture_framebuffer"},
        "coordinate_space": "logical",
        "output": {
            "name": output_name,
            "logical": logical,
            "physical": {"width": 16, "height": 10},
            "requested_scale": {"numerator": 1, "denominator": 1},
            "effective_scale": {"numerator": 1, "denominator": 1},
            "revision": 3,
            "frame_revision": 7,
        },
        "framebuffer": {
            "path": frame_name,
            "format": "png",
            "dimensions": {"width": 16, "height": 10},
            "sha256": frame_hash,
            "clear_color": [0, 0, 0, 255],
            "clear_tolerance": 0,
        },
        "layers": layers,
    }


def run_self_test(root: Path) -> int:
    """Run deterministic positive and negative fixture validations."""

    root.mkdir(parents=True, exist_ok=True)
    width, height = 16, 10
    painted = bytes((30, 50, 70, 255))
    clear = bytes((0, 0, 0, 255))
    good_pixels = painted * (width * height)
    bad_pixels = clear * (width * 3) + painted * (width * (height - 3))
    good_path = root / "framebuffer.png"
    bad_path = root / "framebuffer-clear-edge.png"
    good_path.write_bytes(encode_rgba_png(width, height, good_pixels))
    bad_path.write_bytes(encode_rgba_png(width, height, bad_pixels))

    good_state_path = root / "state.json"
    bad_state_path = root / "state-clear-edge.json"
    good_state = fixture_state(good_path.name, sha256_file(good_path))
    bad_state = fixture_state(bad_path.name, sha256_file(bad_path))
    write_json(good_state_path, good_state)
    write_json(bad_state_path, bad_state)

    fixed_timestamp = "1970-01-01T00:00:00Z"
    positive = evaluate(
        good_state_path,
        good_path,
        allow_fixture=True,
        timestamp=fixed_timestamp,
    )
    negative = evaluate(
        bad_state_path,
        bad_path,
        allow_fixture=True,
        timestamp=fixed_timestamp,
    )
    positive_path = root / "positive-evidence.json"
    negative_path = root / "negative-evidence.json"
    write_json(positive_path, positive)
    write_json(negative_path, negative)
    negative_edge_failure = any(
        "clear/unpainted edge band exceeds 2 pixels" in failure
        for failure in negative["failures"]
    )
    if positive["status"] != "passed" or negative["status"] != "failed" or not negative_edge_failure:
        print("viewport-gate self-test: FAILED", file=sys.stderr)
        print(json.dumps({"positive": positive, "negative": negative}, indent=2), file=sys.stderr)
        return 1
    print("viewport-gate self-test: passed")
    print(f"fixture_root={root}")
    print(f"positive_status={positive['status']} evidence={positive_path}")
    print(f"negative_status={negative['status']} evidence={negative_path}")
    print(f"negative_failure={negative['failures'][0]}")
    return 0


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    """Parse normal and self-test modes."""

    parser = argparse.ArgumentParser(
        description="Validate compositor viewport JSON against a compositor PNG framebuffer."
    )
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--self-test-root",
        type=Path,
        default=Path("target/viewport-gate-self-test"),
    )
    parser.add_argument("--state", type=Path)
    parser.add_argument("--framebuffer", type=Path)
    parser.add_argument("--artifact", type=Path)
    args = parser.parse_args(argv)
    if not args.self_test and (args.state is None or args.framebuffer is None):
        parser.error("normal mode requires --state and --framebuffer")
    if args.self_test and (args.state is not None or args.framebuffer is not None):
        parser.error("--self-test cannot be combined with --state or --framebuffer")
    return args


def run_normal(args: argparse.Namespace) -> int:
    """Evaluate a runtime capture and write its evidence artifact."""

    state_path = args.state
    frame_path = args.framebuffer
    assert state_path is not None and frame_path is not None
    artifact = args.artifact or default_artifact(state_path)
    evidence = evaluate(state_path, frame_path)
    write_json(artifact, evidence)
    print(f"viewport-gate: {evidence['status']}")
    print(f"evidence={artifact}")
    for failure in evidence["failures"]:
        print(f"failure={failure}", file=sys.stderr)
    return 0 if evidence["status"] == "passed" else 1


def main(argv: Sequence[str] | None = None) -> int:
    """Run the viewport gate."""

    args = parse_args(argv or sys.argv[1:])
    if args.self_test:
        return run_self_test(args.self_test_root)
    return run_normal(args)


if __name__ == "__main__":
    sys.exit(main())
