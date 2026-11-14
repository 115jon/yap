"""Converts MediaPipe's selfie_segmenter.tflite to ONNX (NCHW).

Input  "input": float32 [1, 3, 256, 256], RGB in 0..1.
Output "mask":  float32 [1, 1, 256, 256], person confidence 0..1.

Handles exactly the ops this model uses: DEQUANTIZE (fp16 weights, folded),
CONV_2D, DEPTHWISE_CONV_2D, AVERAGE_POOL_2D (global), ADD, MUL, RELU,
HARD_SWISH, LOGISTIC, RESIZE_BILINEAR (half-pixel), and MediaPipe's custom
Convolution2DTransposeBias. Anything else is an error.

Usage: convert.py <in.tflite> <out.onnx>
"""
import struct
import sys

import numpy as np
import onnx
import tflite
from onnx import TensorProto, helper, numpy_helper

src, dst = sys.argv[1], sys.argv[2]
buf = open(src, "rb").read()
m = tflite.Model.GetRootAsModel(buf, 0)
g = m.Subgraphs(0)
OPS = {v: k for k, v in tflite.BuiltinOperator.__dict__.items() if not k.startswith("_")}
SAME = tflite.Padding.SAME


def op_name(op):
    code = m.OperatorCodes(op.OpcodeIndex())
    b = max(code.BuiltinCode(), code.DeprecatedBuiltinCode())
    return OPS[b], code.CustomCode()


def tensor_data(i):
    t = g.Tensors(i)
    b = m.Buffers(t.Buffer())
    if b.DataLength() == 0:
        return None
    raw = b.DataAsNumpy().tobytes()
    dtype = {tflite.TensorType.FLOAT32: np.float32, tflite.TensorType.FLOAT16: np.float16,
             tflite.TensorType.INT32: np.int32}[t.Type()]
    return np.frombuffer(raw, dtype=dtype).reshape(t.ShapeAsNumpy())


def shape(i):
    return list(g.Tensors(i).ShapeAsNumpy())


consts = {}   # tensor index -> float32/int32 numpy (NHWC/OHWI as stored)
nodes, inits = [], []
names = {}    # tensor index -> onnx value name


def name(i):
    return names.setdefault(i, f"t{i}")


def init(arr, label):
    inits.append(numpy_helper.from_array(arr, label))
    return label


def same_pads(in_hw, k_hw, s_hw):
    pads_b, pads_e = [], []
    for n, k, s in zip(in_hw, k_hw, s_hw):
        out = -(-n // s)
        total = max((out - 1) * s + k - n, 0)
        pads_b.append(total // 2)
        pads_e.append(total - total // 2)
    return pads_b + pads_e


def act(x_name, fused, label):
    if fused == tflite.ActivationFunctionType.NONE:
        return x_name
    if fused == tflite.ActivationFunctionType.RELU:
        nodes.append(helper.make_node("Relu", [x_name], [label + "_act"]))
        return label + "_act"
    raise SystemExit(f"fused activation {fused} not handled")


inp = g.Inputs(0)
out_index = g.Outputs(0)
names[inp] = "input"

for k in range(g.OperatorsLength()):
    op = g.Operators(k)
    kind, custom = op_name(op)
    ins = [op.Inputs(j) for j in range(op.InputsLength())]
    outs = [op.Outputs(j) for j in range(op.OutputsLength())]
    o = op.BuiltinOptions()
    label = f"n{k}"
    if kind == "DEQUANTIZE":
        consts[outs[0]] = tensor_data(ins[0]).astype(np.float32)
        continue
    w = lambda j: consts.get(ins[j]) if ins[j] in consts else tensor_data(ins[j])
    if kind == "CONV_2D":
        x = tflite.Conv2DOptions(); x.Init(o.Bytes, o.Pos)
        weight = w(1).transpose(0, 3, 1, 2)  # OHWI -> OIHW
        bias = w(2)
        _, h, wd, _ = shape(ins[0])
        kh, kw = weight.shape[2:]
        pads = same_pads((h, wd), (kh, kw), (x.StrideH(), x.StrideW())) if x.Padding() == SAME else [0, 0, 0, 0]
        y = label + "_y"
        nodes.append(helper.make_node("Conv", [name(ins[0]), init(weight, label + "_w"), init(bias, label + "_b")], [y],
                                      kernel_shape=[kh, kw], strides=[x.StrideH(), x.StrideW()], pads=pads,
                                      dilations=[x.DilationHFactor(), x.DilationWFactor()]))
        names[outs[0]] = act(y, x.FusedActivationFunction(), label)
    elif kind == "DEPTHWISE_CONV_2D":
        x = tflite.DepthwiseConv2DOptions(); x.Init(o.Bytes, o.Pos)
        assert x.DepthMultiplier() == 1
        weight = w(1).transpose(3, 0, 1, 2)  # 1HWC -> C1HW
        bias = w(2)
        _, h, wd, c = shape(ins[0])
        kh, kw = weight.shape[2:]
        pads = same_pads((h, wd), (kh, kw), (x.StrideH(), x.StrideW())) if x.Padding() == SAME else [0, 0, 0, 0]
        y = label + "_y"
        nodes.append(helper.make_node("Conv", [name(ins[0]), init(weight, label + "_w"), init(bias, label + "_b")], [y],
                                      kernel_shape=[kh, kw], strides=[x.StrideH(), x.StrideW()], pads=pads, group=c,
                                      dilations=[x.DilationHFactor(), x.DilationWFactor()]))
        names[outs[0]] = act(y, x.FusedActivationFunction(), label)
    elif kind == "AVERAGE_POOL_2D":
        x = tflite.Pool2DOptions(); x.Init(o.Bytes, o.Pos)
        _, h, wd, _ = shape(ins[0])
        assert (x.FilterHeight(), x.FilterWidth()) == (h, wd), "only global pools"
        y = label + "_y"
        nodes.append(helper.make_node("GlobalAveragePool", [name(ins[0])], [y]))
        names[outs[0]] = act(y, x.FusedActivationFunction(), label)
    elif kind in ("ADD", "MUL"):
        x = (tflite.AddOptions if kind == "ADD" else tflite.MulOptions)(); x.Init(o.Bytes, o.Pos)
        operands = []
        for j in (0, 1):
            if ins[j] in consts or tensor_data(ins[j]) is not None:
                c = w(j)
                if c.ndim == 4:
                    c = c.transpose(0, 3, 1, 2)
                operands.append(init(np.ascontiguousarray(c, dtype=np.float32), f"{label}_c{j}"))
            else:
                operands.append(name(ins[j]))
        y = label + "_y"
        nodes.append(helper.make_node("Add" if kind == "ADD" else "Mul", operands, [y]))
        names[outs[0]] = act(y, x.FusedActivationFunction(), label)
    elif kind in ("RELU", "HARD_SWISH", "LOGISTIC"):
        onnx_op = {"RELU": "Relu", "HARD_SWISH": "HardSwish", "LOGISTIC": "Sigmoid"}[kind]
        nodes.append(helper.make_node(onnx_op, [name(ins[0])], [label + "_y"]))
        names[outs[0]] = label + "_y"
    elif kind == "RESIZE_BILINEAR":
        x = tflite.ResizeBilinearOptions(); x.Init(o.Bytes, o.Pos)
        assert x.HalfPixelCenters() and not x.AlignCorners()
        size = tensor_data(ins[1]).astype(np.int64)
        c = shape(ins[0])[3]
        sizes = init(np.array([1, c, size[0], size[1]], dtype=np.int64), label + "_sizes")
        nodes.append(helper.make_node("Resize", [name(ins[0]), "", "", sizes], [label + "_y"], mode="linear",
                                      coordinate_transformation_mode="half_pixel"))
        names[outs[0]] = label + "_y"
    elif kind == "CUSTOM" and custom == b"Convolution2DTransposeBias":
        raw = op.CustomOptionsAsNumpy().tobytes()
        padding, stride_w, stride_h = struct.unpack("<iii", raw[:12])
        weight = w(1)            # O, kh, kw, I
        bias = w(2)
        kh, kw = weight.shape[1:3]
        # SAME (1) with kernel == stride needs no padding; anything else is
        # not this model.
        assert padding in (1, 2) and (kh, kw) == (stride_h, stride_w), (padding, kh, kw, stride_h, stride_w)
        onnx_w = weight.transpose(3, 0, 1, 2)  # I, O, kh, kw
        nodes.append(helper.make_node("ConvTranspose", [name(ins[0]), init(np.ascontiguousarray(onnx_w), label + "_w"),
                                                        init(bias, label + "_b")], [label + "_y"],
                                      kernel_shape=[kh, kw], strides=[stride_h, stride_w], pads=[0, 0, 0, 0]))
        names[outs[0]] = label + "_y"
    else:
        raise SystemExit(f"op {kind} {custom} not handled")

# The graph's output is the last op's value; rename it "mask".
nodes.append(helper.make_node("Identity", [name(out_index)], ["mask"]))
graph = helper.make_graph(
    nodes, "selfie_segmenter",
    [helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 3, 256, 256])],
    [helper.make_tensor_value_info("mask", TensorProto.FLOAT, [1, 1, 256, 256])],
    initializer=inits)
model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)],
                          producer_name="yap-native convert.py (from MediaPipe selfie_segmenter.tflite)")
model.ir_version = 8
onnx.checker.check_model(model)
onnx.save(model, dst)
print(f"{len(nodes)} nodes, {len(inits)} initializers -> {dst}")
