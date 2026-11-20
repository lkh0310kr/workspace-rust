#!/usr/bin/env python3
"""고정 시나리오 + fuzz 시나리오 생성. ids 는 원본 mk() 의 DFS 순서와 같다."""
import json, random, os
D = json.load(open(os.path.join(os.path.dirname(__file__), "../data/data.json")))
names, kids, par = [], [], []
def mk(d, p):
    i = len(names); names.append(d if isinstance(d, str) else d[0]); kids.append([]); par.append(p)
    if isinstance(d, list):
        for c in d[1:]: kids[i].append(mk(c, i))
    return i
mk(D, None)
byname = {}
for i, n in enumerate(names): byname.setdefault(n, []).append(i)
nid = lambda n: byname[n][0]
OUT = os.path.join(os.path.dirname(__file__), "scenarios")
os.makedirs(OUT, exist_ok=True)

def save(name, actions): json.dump({"name": name, "actions": actions}, open(f"{OUT}/{name}.json", "w"))
def steps(n, ms=16, every=None, label="f"):
    a = []
    if every is None: return [{"a": "step", "ms": ms, "n": n}]
    done = 0
    while done < n:
        k = min(every, n - done); a += [{"a": "step", "ms": ms, "n": k}, {"a": "dump", "label": f"{label}{done+k}"}]; done += k
    return a

# 1. 초기 로드(전환 애니메이션)
save("initial", [{"a": "dump", "label": "t0"}] + steps(70, every=1, label="f"))
# 2. 하위로 이동 / 뒤로
fe, react, lang = nid("프론트엔드"), nid("React"), nid("언어")
save("navigate", steps(60) + [{"a": "dump", "label": "root"}, {"a": "node", "id": fe}] + steps(70, every=2, label="fe") +
     [{"a": "node", "id": react}] + steps(70, every=3, label="react") + [{"a": "node", "id": react}] + steps(60, every=4, label="back") +
     [{"a": "key", "key": "Escape"}] + steps(60, every=5, label="esc"))
# 3. 다이얼(자식 10개 초과): 휠/키/드래그/반동
wheel_seq = []
for i in range(14): wheel_seq += [{"a": "wheel", "x": 550, "y": 120, "dy": 25}, {"a": "step", "ms": 50, "n": 1}, {"a": "dump", "label": f"w{i}"}]
key_seq = []
for i in range(40): key_seq += [{"a": "key", "key": "ArrowDown"}, {"a": "step", "ms": 16, "n": 3}, {"a": "dump", "label": f"kd{i}"}]
for i in range(45): key_seq += [{"a": "key", "key": "ArrowUp"}, {"a": "step", "ms": 16, "n": 3}, {"a": "dump", "label": f"ku{i}"}]
drag_seq = [{"a": "down", "x": 550, "y": 140}]
import math
for i in range(30):
    ang = -90 + i * 4; drag_seq += [{"a": "move", "x": 550 + 250 * math.cos(math.radians(ang)), "y": 432 + 250 * math.sin(math.radians(ang))}, {"a": "step", "ms": 16, "n": 2}, {"a": "dump", "label": f"dr{i}"}]
drag_seq += [{"a": "up", "click": None}] + steps(40, every=8, label="dr_end")
save("dial", steps(60) + [{"a": "node", "id": lang}] + steps(60) + wheel_seq + steps(30, every=10, label="aw") + key_seq + drag_seq)
# 4. fuzz
def fuzz(seed, n=260):
    r = random.Random(seed); a = steps(40); focus = 0; stack = [0]; W, H = 1100, 800
    for i in range(n):
        k = r.random()
        if k < 0.30:
            cand = kids[stack[-1]] if kids[stack[-1]] else []
            if cand and r.random() < 0.8:
                t = r.choice(cand); a.append({"a": "node", "id": t}); stack.append(t)
            elif len(stack) > 1: a.append({"a": "node", "id": stack[-1]}); stack.pop()
            else: a.append({"a": "node", "id": r.randrange(len(names))})
        elif k < 0.50: a.append({"a": "wheel", "x": r.uniform(0, W), "y": r.uniform(0, H), "dy": r.choice([-40, -20, -8, 8, 20, 40, 60])})
        elif k < 0.62: a.append({"a": "key", "key": r.choice(["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight", "Escape", "Backspace", "Tab", "a"])})
        elif k < 0.72:
            a += [{"a": "down", "x": r.uniform(100, 1000), "y": r.uniform(100, 700)}]
            for _ in range(r.randint(1, 6)): a += [{"a": "move", "x": r.uniform(100, 1000), "y": r.uniform(100, 700)}, {"a": "step", "ms": 16, "n": r.randint(0, 3)}]
            a.append({"a": "up", "click": None})
        elif k < 0.76: a.append({"a": "nodekey", "id": r.randrange(len(names)), "key": r.choice(["Enter", " ", "x"])})
        a.append({"a": "step", "ms": r.choice([8, 16, 16, 16, 33, 50, 100]), "n": r.randint(1, 12)})
        if i % 3 == 0: a.append({"a": "dump", "label": f"z{i}"})
    a.append({"a": "dump", "label": "end"}); return a
for s in range(1, 9): save(f"fuzz{s}", fuzz(s))
print("scenarios:", sorted(os.listdir(OUT)))
print("ids:", {"프론트엔드": fe, "React": react, "언어": lang, "총노드": len(names)})
