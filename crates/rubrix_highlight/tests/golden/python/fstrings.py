name = 'x'
value = 3.14159
d = {'k': 1}
a = f'plain {name} text'
b = f"nested {d['k']} quotes"
c = f"same {d["k"]} quote"
e = f'{value:.2f} and {value!r} and {name!s:>10}'
g = f'{{literal}} {name} {{{name}}}'
h = f"{value:{width}.{precision}}"
i = f"{f'inner {name}'}"
j = f"{f"{f'{name}'}"}"
k = f'{(lambda y: y + 1)(2)}'
l = f'{name = }'
m = f'{ {"a": 1}["a"] }'
n = f"{[x for x in range(3) if x]}"
o = f'{name:{"^"}{10}}'
p = f"""multi {name}
line {d['k']} text {
    value
} tail
"""
q = rf'{name}\d{{2}}' + fr"\w{name}"
r = f'{name}' f"{value}" 'plain'
s = f'{d["k"]!r:>{10}}'
t = f"{3 if name else 4}"
u = f"{(y := 5)}"
