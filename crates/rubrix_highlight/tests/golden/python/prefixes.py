a = 'single' + "double" + ''
b = r'raw\n' + R"RAW\d" + u'text' + U"TEXT"
c = b'bytes' + B"BYTES" + rb'raw\x00' + Rb"mixed" + bR'swap' + BR"both"
d = f'format {a}' + F"FORMAT {b}" + rf'{c}\d' + fR"{d}\w" + Fr'{e}'
e = t'template {a}' + T"TEMPLATE {b}" + tr'{c}\s' + rT"{d}"
f = 'it\'s' + "say \"hi\"" + 'tab\there' + "nl\n" + '\x41\u00e9\U0001F600'
g = '\N{GREEK SMALL LETTER ALPHA}' + '\101\7\0' + r'\\' + "x"
h = 'a' 'b' "c" f'd{e}'
