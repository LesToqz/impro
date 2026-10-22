#!/usr/bin/env python3
"""Exact RGBA comparison; PNG codec falls back to the Python standard library."""
import argparse
import json
from pathlib import Path
import struct
import zlib


def read_png(path):
    try:
        from PIL import Image
        im = Image.open(path).convert('RGBA')
        return im.width, im.height, im.tobytes()
    except ImportError:
        raw = Path(path).read_bytes()
        if raw[:8] != b'\x89PNG\r\n\x1a\n':
            raise ValueError('not a PNG')
        pos, data = 8, bytearray()
        while pos < len(raw):
            size = struct.unpack('>I', raw[pos:pos+4])[0]
            kind, block = raw[pos+4:pos+8], raw[pos+8:pos+8+size]
            pos += size + 12
            if kind == b'IHDR':
                w, h, depth, color, _, _, interlace = struct.unpack('>IIBBBBB', block)
            if kind == b'IDAT':
                data.extend(block)
        if depth != 8 or color not in (2, 6) or interlace:
            raise ValueError('fallback supports non-interlaced RGB/RGBA 8-bit PNG')
        channels = 3 if color == 2 else 4
        decoded, previous, result = zlib.decompress(data), bytearray(w*channels), bytearray()
        stride = w * channels
        for y in range(h):
            method = decoded[y*(stride+1)]
            row = bytearray(decoded[y*(stride+1)+1:(y+1)*(stride+1)])
            for x in range(stride):
                a = row[x-channels] if x >= channels else 0
                b = previous[x]
                c = previous[x-channels] if x >= channels else 0
                p = a+b-c
                distances = [abs(p-a), abs(p-b), abs(p-c)]
                paeth = (a, b, c)[distances.index(min(distances))]
                predictor = (0, a, b, (a+b)//2, paeth)[method]
                row[x] = (row[x]+predictor) & 255
            if channels == 4:
                result.extend(row)
            else:
                for x in range(0, stride, 3):
                    result.extend(row[x:x+3] + b'\xff')
            previous = row
        return w, h, bytes(result)


def write_png(path, w, h, rgba):
    def chunk(kind, block):
        return struct.pack('>I', len(block)) + kind + block + struct.pack('>I', zlib.crc32(kind+block))
    rows = b''.join(b'\0'+rgba[y*w*4:(y+1)*w*4] for y in range(h))
    Path(path).write_bytes(b'\x89PNG\r\n\x1a\n' +
        chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0)) +
        chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def compare(reference, actual, output):
    w, h, a = read_png(reference)
    aw, ah, b = read_png(actual)
    if (w, h) != (aw, ah):
        raise ValueError(f'image size mismatch: {(w, h)} != {(aw, ah)}')
    changed, error, bounds, tiles = 0, 0, [w, h, -1, -1], set()
    diff = bytearray(w*h*4)
    for i in range(w*h):
        offset = i*4
        delta = [abs(a[offset+c]-b[offset+c]) for c in range(4)]
        error += sum(delta)
        diff[offset:offset+4] = bytes(delta[:3] + [255])
        if any(delta):
            changed += 1
            x, y = i % w, i // w
            bounds = [min(bounds[0], x), min(bounds[1], y), max(bounds[2], x), max(bounds[3], y)]
            tiles.add((x//32, y//32))
    # Connected occupied 32px tiles provide compact regional bounding boxes.
    regions = []
    while tiles:
        seed = tiles.pop()
        pending, group = [seed], [seed]
        while pending:
            x, y = pending.pop()
            for neighbor in [(x-1,y), (x+1,y), (x,y-1), (x,y+1)]:
                if neighbor in tiles:
                    tiles.remove(neighbor)
                    pending.append(neighbor)
                    group.append(neighbor)
        regions.append([min(x for x,y in group)*32, min(y for x,y in group)*32,
                        min(w, (max(x for x,y in group)+1)*32),
                        min(h, (max(y for x,y in group)+1)*32)])
    output = Path(output)
    write_png(str(output)+'-diff.png', w, h, diff)
    side = b''.join(a[y*w*4:(y+1)*w*4]+b[y*w*4:(y+1)*w*4] for y in range(h))
    write_png(str(output)+'-side.png', w*2, h, side)
    return dict(changed=changed, total=w*h, bbox=(bounds[:2]+[bounds[2]+1,bounds[3]+1]) if changed else None,
                regions=sorted(regions), mean_channel_error=error/(w*h*4), size=[w,h])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference')
    parser.add_argument('actual')
    parser.add_argument('output')
    args = parser.parse_args()
    print(json.dumps(compare(args.reference, args.actual, args.output), sort_keys=True))
