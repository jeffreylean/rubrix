# a comment
x = 1  # trailing comment
#no space
#!shebang style comment
y = "# not a comment" + '# nor this'
z = [1,  # one
     2]  # two
total = 1 + \
    2 + \
    3
text = "line one \
line two \
line three"
raw = r"raw \
continues"
single = 'ends with a backslash \\'
if True: pass; x = 1; y = 2
assert x, \
    "message"
with open('a') as f, \
     open('b') as g:
    pass
# comment with "quotes" and 'quotes' and """triple"""
