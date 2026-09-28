// C++ test program for binviz fixtures (GCC-produced DWARF, Itanium mangling).
#include <cmath>
#include <cstdio>
#include <vector>

namespace geo {

struct Point {
    double x;
    double y;
};

class Shape {
public:
    virtual ~Shape() = default;
    virtual double area() const = 0;
    virtual const char *name() const = 0;
};

class Circle : public Shape {
public:
    explicit Circle(Point c, double r) : center(c), radius(r) {}
    double area() const override { return M_PI * radius * radius; }
    const char *name() const override { return "circle"; }

private:
    Point center;
    double radius;
};

class Rect : public Shape {
public:
    Rect(Point a, Point b) : min(a), max(b) {}
    double area() const override { return std::fabs(max.x - min.x) * std::fabs(max.y - min.y); }
    const char *name() const override { return "rect"; }

private:
    Point min, max;
};

template <typename T>
T clamp(T v, T lo, T hi) {
    return v < lo ? lo : (v > hi ? hi : v);
}

}  // namespace geo

static int g_counter = 0;
const char *const kBanner = "binviz C++ fixture";

__attribute__((noinline)) double total_area(const std::vector<geo::Shape *> &shapes) {
    double sum = 0.0;
    for (const geo::Shape *s : shapes) {
        sum += s->area();
        ++g_counter;
    }
    return sum;
}

int main() {
    geo::Circle c({0.0, 0.0}, 2.0);
    geo::Rect r({1.0, 1.0}, {4.0, 3.0});
    std::vector<geo::Shape *> shapes{&c, &r};
    double area = total_area(shapes);
    std::printf("%s: %d shapes, area %.2f, clamped %d\n", kBanner, g_counter, area,
                geo::clamp(g_counter, 0, 1));
    return 0;
}
