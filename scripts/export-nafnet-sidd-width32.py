"""Deterministically export the pinned NAFNet-SIDD-width32 checkpoint.

This build helper imports the architecture from an exact upstream checkout instead of
vendoring model code into Starroom. The generated ONNX stays in models/local/ and is
never committed or packaged.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import subprocess
import sys
import types
from collections import OrderedDict
from pathlib import Path

import onnx
import torch


UPSTREAM_REVISION = "2b4af71ebe098a92a75910c233a3965a3e93ede4"
CHECKPOINT_SHA256 = "89c70e808d1783b6c07911306e106aaf0d4f7f3da8c61078b99ff7f8929a26f4"
MODEL_EDGE = 512
OPSET = 20


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_module(name: str, path: Path) -> types.ModuleType:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {name} from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def load_upstream_architecture(upstream: Path):
    revision = subprocess.check_output(
        ["git", "-c", f"safe.directory={upstream.as_posix()}", "-C", str(upstream), "rev-parse", "HEAD"],
        text=True,
    ).strip()
    if revision != UPSTREAM_REVISION:
        raise RuntimeError(f"expected NAFNet {UPSTREAM_REVISION}, found {revision}")

    arch_root = upstream / "basicsr" / "models" / "archs"
    for package in ("basicsr", "basicsr.models", "basicsr.models.archs"):
        module = types.ModuleType(package)
        module.__path__ = []  # type: ignore[attr-defined]
        sys.modules[package] = module
    utils = types.ModuleType("basicsr.utils")
    utils.get_root_logger = lambda: None
    sys.modules["basicsr.utils"] = utils

    load_module("basicsr.models.archs.arch_util", arch_root / "arch_util.py")
    load_module("basicsr.models.archs.local_arch", arch_root / "local_arch.py")
    return load_module("basicsr.models.archs.NAFNet_arch", arch_root / "NAFNet_arch.py").NAFNet


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--upstream", required=True, type=Path)
    parser.add_argument("--checkpoint", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    actual_checkpoint_hash = sha256(args.checkpoint)
    if actual_checkpoint_hash != CHECKPOINT_SHA256:
        raise RuntimeError(
            f"checkpoint hash mismatch: expected {CHECKPOINT_SHA256}, "
            f"found {actual_checkpoint_hash}"
        )

    nafnet = load_upstream_architecture(args.upstream)
    model = nafnet(
        img_channel=3,
        width=32,
        middle_blk_num=12,
        enc_blk_nums=[2, 2, 4, 8],
        dec_blk_nums=[2, 2, 2, 2],
    )
    checkpoint = torch.load(args.checkpoint, map_location="cpu", weights_only=False)
    state = checkpoint.get("params", checkpoint.get("params_ema", checkpoint))
    model.load_state_dict(
        OrderedDict((key.removeprefix("module."), value) for key, value in state.items()),
        strict=True,
    )
    model.eval()

    args.output.parent.mkdir(parents=True, exist_ok=True)
    torch.manual_seed(0)
    sample = torch.zeros(1, 3, MODEL_EDGE, MODEL_EDGE, dtype=torch.float32)
    with torch.inference_mode():
        torch.onnx.export(
            model,
            sample,
            args.output,
            input_names=["input"],
            output_names=["output"],
            opset_version=OPSET,
            do_constant_folding=True,
            dynamo=False,
        )

    exported = onnx.load(args.output)
    # Upstream crops with Python-derived H/W values, which the legacy exporter records as
    # symbolic Slice dimensions even though this graph has a fixed 512 input. Publish the
    # verified static contract explicitly so ORT consumers never guess the tile shape.
    output_shape = exported.graph.output[0].type.tensor_type.shape
    for dimension, value in zip(output_shape.dim, (1, 3, MODEL_EDGE, MODEL_EDGE), strict=True):
        dimension.ClearField("dim_param")
        dimension.dim_value = value
    onnx.checker.check_model(exported)
    onnx.save(exported, args.output)
    input_dims = [dim.dim_value for dim in exported.graph.input[0].type.tensor_type.shape.dim]
    output_dims = [dim.dim_value for dim in output_shape.dim]
    expected_dims = [1, 3, MODEL_EDGE, MODEL_EDGE]
    if exported.graph.input[0].name != "input" or input_dims != expected_dims:
        raise RuntimeError(f"invalid ONNX input contract: {exported.graph.input[0].name} {input_dims}")
    if exported.graph.output[0].name != "output" or output_dims != expected_dims:
        raise RuntimeError(
            f"invalid ONNX output contract: {exported.graph.output[0].name} {output_dims}"
        )
    print(f"checkpoint_sha256={actual_checkpoint_hash}")
    print(f"onnx_sha256={sha256(args.output)}")
    print(f"onnx_bytes={args.output.stat().st_size}")


if __name__ == "__main__":
    main()
