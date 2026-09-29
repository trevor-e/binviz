// C++ classes with virtual functions and run-time type information, for the
// MSVC ABI (clang, lld-link, no C runtime; see scripts/build-fixtures.sh), 32
// and 64-bit: single and multiple inheritance, a namespace, a template. The
// type_info vtable the type descriptors point at comes from the C runtime,
// which isn't linked: a stand-in takes its name.

namespace shapes {
struct Shape {
    int id;
    virtual int area() const { return 0; }
    virtual int sides() const { return 0; }
};

struct Square : Shape {
    int s;
    int area() const override { return s * s; }
    int sides() const override { return 4; }
};
}

struct Named {
    virtual const char *name() const { return "named"; }
};

// Two bases, so two vtables: Label's own (for Square) and the one for Named.
struct Label : shapes::Square, Named {
    const char *name() const override { return "label"; }
    int area() const override { return s + 1; }
};

template <typename T> struct Box {
    T v;
    virtual T get() const { return v; }
};

extern "C" const void *fake_type_info_vftable[1] = {0};

__declspec(noinline) static const shapes::Shape *pick(const shapes::Shape *a, const shapes::Shape *b, int k) {
    return k & 1 ? a : b;
}

extern "C" __declspec(dllexport) int make(int k) {
    shapes::Square sq;
    sq.s = k;
    Label lb;
    lb.s = k;
    Box<int> b;
    b.v = k;
    const shapes::Shape *s = pick(&sq, &lb, k);
    const Named *n = &lb;
    const Box<int> *bp = k > 3 ? &b : nullptr;
    return s->area() + s->sides() + n->name()[0] + (bp ? bp->get() : 0);
}
