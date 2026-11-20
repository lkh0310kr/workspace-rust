#!/usr/bin/env python3
"""원본(CEF) 이미지와 gpui 캡처를 비교한다. 사용: tools_diff.py ref.png gpui.png out_prefix
   출력: <prefix>-side.png(좌 원본/우 gpui), <prefix>-diff.png(차이 히트맵), 콘솔에 요약(평균 차이, 5%/15% 초과 픽셀 비율)."""
import sys
from PIL import Image, ImageChops
a, b, out = Image.open(sys.argv[1]).convert("RGB"), Image.open(sys.argv[2]).convert("RGB"), sys.argv[3]
if a.size != b.size:
    print(f"크기 다름: ref={a.size} gpui={b.size} → gpui 를 ref 크기로 맞춰 비교(리샘플링 주의)")
    b = b.resize(a.size)
d = ImageChops.difference(a, b).convert("L")
px = list(d.getdata()); n = len(px)
print(f"평균 차이 {sum(px)/n/255*100:.3f}%  | 차이>5% 픽셀 {sum(1 for v in px if v>13)/n*100:.2f}%  | 차이>15% 픽셀 {sum(1 for v in px if v>38)/n*100:.2f}%")
side = Image.new("RGB", (a.width * 2 + 10, a.height), (128, 128, 128)); side.paste(a, (0, 0)); side.paste(b, (a.width + 10, 0)); side.save(out + "-side.png")
d.point(lambda v: min(255, v * 4)).save(out + "-diff.png")
