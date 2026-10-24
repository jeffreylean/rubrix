"""Module docstring.

It spans lines and holds "quotes" and 'quotes'.
"""

'''Single-quote style
  also spans lines'''

def documented(x):
    """Return x.

    Indented body with a backslash escape \n and an escaped end \""" inside.
    """
    return x

text = """one
two"""
more = '''first

third (the line above is empty)'''
raw = r"""raw \n
still raw \"""" + b"""bytes
over lines""" + rb'''raw bytes
\x00 again'''
inline = """a""" + '''b''' + """c "quoted" d""" + '''e 'quoted' f'''
four = """a"" b"""
closing = """text"""; after = 1
empty = """""" + ''''''
mixed = """has ''' inside""" + '''has """ inside'''
