// Original benchmark adapter. Upstream headers are retained separately.
// Host translation unit supplies Point = std::array<double, 3> and squared().
#ifndef RILL_KDTREE_PLUSPLUS_ADAPTER_HPP
#define RILL_KDTREE_PLUSPLUS_ADAPTER_HPP

#include <cfenv>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <memory>
#include <stdexcept>
#include <type_traits>
#include <utility>
#include <vector>
#include "../../vendor/cpp-kdtree-plusplus/kdtree++/kdtree.hpp"

namespace rill_kdtree_plusplus {

static_assert(std::numeric_limits<double>::is_iec559 &&
                  std::numeric_limits<double>::radix == 2 &&
                  std::numeric_limits<double>::digits == 53,
              "the adapter requires IEEE binary64");

// The library accepts custom accessors/comparators. A stable ID breaks ties
// without perturbing coordinates. This prevents coincident points producing a
// one-sided insertion chain during native median optimisation.
struct Key {
    double coordinate = 0.0;
    std::uint64_t id = 0;
};
struct KeyLess {
    bool operator()(const Key& a, const Key& b) const {
        if (a.coordinate < b.coordinate) return true;
        if (b.coordinate < a.coordinate) return false;
        return a.id < b.id;
    }
};
struct Record {
    Point point{};
    std::uint64_t id = 0;
};
struct Accessor {
    using result_type = Key;
    Key operator()(const Record& record, std::size_t dimension) const {
        return Key{record.point[dimension], record.id};
    }
};
// Range traversal does not call the distance functor. Supplying the regular
// double difference keeps the native class's distance_type well-defined.
struct Distance {
    using distance_type = double;
    double operator()(const Key& a, const Key& b) const {
        const double delta = a.coordinate - b.coordinate;
        return delta * delta;
    }
};
using Tree = KDTree::KDTree<3, Record, Accessor, Distance, KeyLess>;

inline bool finite_point(const Point& point) {
    for (double x : point) {
        if (!std::isfinite(x)) return false;
    }
    return true;
}

inline bool in_radius(const Point& point, const Point& centre, double radius) {
    if (radius == 0.0) {
        return point[0] == centre[0] && point[1] == centre[1] &&
               point[2] == centre[2];
    }
    if (radius < 1e-150 || radius > 1e150) {
        double distance = 0.0;
        for (std::size_t dimension = 0; dimension < 3; ++dimension) {
            distance = std::hypot(distance, point[dimension] - centre[dimension]);
        }
        return distance <= radius;
    }
    return ::squared(point, centre) <= radius * radius;
}

// Upstream copies AND assigns visitors while unwinding traversal. Pointer
// members keep this functor copy-assignable in C++17 without type erasure or
// extra allocations. The referenced arguments outlive the synchronous query.
struct ReportVisitor {
    const Point* centre;
    double radius;
    std::vector<std::uint64_t>* out;

    void operator()(const Record& record) const {
        if (in_radius(record.point, *centre, radius)) {
            out->push_back(record.id);
        }
    }
};
static_assert(std::is_copy_assignable<ReportVisitor>::value,
              "native range traversal assigns its visitor");

// For normal contract radii, each accepted rounded component square is <= R.
// nextUp(R) bounds its exact pre-rounding value. The first nextUp after sqrt
// covers sqrt rounding, and the second covers subtraction rounding. Finally
// round the coordinate endpoints outward. See README for assumptions/proof.
inline bool closed_region(const Point& centre, double radius,
                          Tree::_Region_& region) {
    if (radius < 1e-150 || radius > 1e150 ||
        std::fegetround() != FE_TONEAREST) {
        return false;
    }
    const double positive_infinity = std::numeric_limits<double>::infinity();
    const double negative_infinity = -positive_infinity;
    const double squared_radius = radius * radius;
    const double squared_upper = std::nextafter(squared_radius, positive_infinity);
    double half_width = std::nextafter(std::sqrt(squared_upper), positive_infinity);
    half_width = std::nextafter(half_width, positive_infinity);
    for (std::size_t dimension = 0; dimension < 3; ++dimension) {
        const double low = std::nextafter(centre[dimension] - half_width,
                                         negative_infinity);
        const double high = std::nextafter(centre[dimension] + half_width,
                                          positive_infinity);
        if (!std::isfinite(low) || !std::isfinite(high)) return false;
        region._M_low_bounds[dimension] = Key{low, 0};
        region._M_high_bounds[dimension] =
            Key{high, std::numeric_limits<std::uint64_t>::max()};
    }
    return true;
}

}  // namespace rill_kdtree_plusplus

class Native {
    std::vector<Point> points;
    std::unique_ptr<rill_kdtree_plusplus::Tree> tree;
    bool dirty = false;

    void rebuild() {
        std::vector<rill_kdtree_plusplus::Record> rows;
        rows.reserve(points.size());
        for (std::size_t i = 0; i < points.size(); ++i) {
            rows.push_back({points[i], static_cast<std::uint64_t>(i)});
        }
        auto replacement = std::make_unique<rill_kdtree_plusplus::Tree>();
        replacement->efficient_replace_and_optimise(rows);
        if (replacement->size() != points.size()) {
            throw std::runtime_error("kdtree-plusplus lost records during rebuild");
        }
        tree = std::move(replacement);
        dirty = false;
    }

    void scan(const Point& centre, double radius,
              std::vector<std::uint64_t>& out) const {
        for (std::size_t i = 0; i < points.size(); ++i) {
            if (rill_kdtree_plusplus::in_radius(points[i], centre, radius)) {
                out.push_back(static_cast<std::uint64_t>(i));
            }
        }
    }

public:
    Native(const std::vector<Point>& input, double /* cell_size */) : points(input) {
        for (const auto& point : points) {
            if (!rill_kdtree_plusplus::finite_point(point)) {
                throw std::invalid_argument("nonfinite kdtree-plusplus point");
            }
        }
        rebuild();
    }

    void update(const std::vector<std::pair<std::uint32_t, Point>>& updates) {
        // Validate the complete update set before mutating authoritative data.
        for (const auto& update : updates) {
            if (update.first >= points.size() ||
                !rill_kdtree_plusplus::finite_point(update.second)) {
                throw std::invalid_argument("invalid kdtree-plusplus update");
            }
        }
        for (const auto& update : updates) points[update.first] = update.second;
        dirty = dirty || !updates.empty();
    }

    // Multiple update calls coalesce into one charged static rebuild/optimise.
    void prepare() {
        if (dirty) rebuild();
    }

    void query(const Point& centre, double radius,
               std::vector<std::uint64_t>& out) const {
        if (!rill_kdtree_plusplus::finite_point(centre) ||
            !std::isfinite(radius) || radius < 0.0) {
            throw std::invalid_argument("invalid kdtree-plusplus query");
        }
        out.clear();
        if (points.empty()) return;
        rill_kdtree_plusplus::Tree::_Region_ region;
        if (dirty || !rill_kdtree_plusplus::closed_region(centre, radius, region)) {
            scan(centre, radius, out);
            return;
        }
        tree->visit_within_range(region,
            rill_kdtree_plusplus::ReportVisitor{&centre, radius, &out});
    }
};

#endif  // RILL_KDTREE_PLUSPLUS_ADAPTER_HPP
