import this
from x import y
if a: pass
elif b: pass
else: pass
for i in range(10):
    if i: continue
    else: break
while True: break
try:
    raise ValueError
except (TypeError, KeyError) as e:
    pass
finally:
    pass
with a as b: yield
def f():
    global g
    nonlocal h
    del i
    assert j
    return k
async def a():
    await x
lambda: None
x = None
y = True or False
z = NotImplemented, Ellipsis, ...
s = self.cls + cls
p = print, abs, all, any, sorted, enumerate, zip, map, filter
t = int, str, float, list, tuple, dict, set, bool, object
u = obj.print + obj.int + obj.None_
