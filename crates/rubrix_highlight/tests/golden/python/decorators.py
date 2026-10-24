@cache
def f(x=1_000, y=2j): pass  # done


@functools.lru_cache(maxsize=128)
async def g():
    pass


@app.route("/path", methods=["GET"])
def route():
    ...


class C:
    @property
    def p(self):
        return self._p

    @p.setter
    def p(self, value):
        self._p = value

    @staticmethod
    @contextlib.contextmanager
    def s():
        yield

    def m(self, a, b):
        return a @ b

x = a @ b
y = (a
     @ b)
z @= w
@ spaced
def h(): ...
