#include <array>
#include <vector>
#include <fstream>
#include <iostream>
#include <chrono>
#include <algorithm>
#include <cmath>
#include <memory>
#include <cstdint>
#include <stdexcept>
#include <functional>
using Point=std::array<double,3>;
double squared(const Point&a,const Point&b){double x=a[0]-b[0],y=a[1]-b[1],z=a[2]-b[2];return x*x+y*y+z*z;}
struct Query{Point point;double radius;};
struct Frame{std::vector<std::pair<uint32_t,Point>> updates;std::vector<Query> queries;};
struct Fixture{std::string name;double cell_size;std::vector<Point> points;std::vector<Frame> frames;};
template<class T>T read(std::ifstream&f){T v;f.read(reinterpret_cast<char*>(&v),sizeof(T));if(!f)throw std::runtime_error("truncated fixture");return v;}
Fixture load(const char*path){std::ifstream in(path,std::ios::binary);char magic[8];in.read(magic,8);if(std::string(magic,8)!="RILLv001")throw std::runtime_error("fixture ABI");Fixture f;auto len=read<uint32_t>(in);f.name.resize(len);in.read(&f.name[0],len);f.cell_size=read<double>(in);auto n=read<uint32_t>(in),nf=read<uint32_t>(in);for(uint32_t i=0;i<n;i++){Point p;for(auto&x:p)x=read<double>(in);f.points.push_back(p);}for(uint32_t i=0;i<nf;i++){Frame frame;auto nu=read<uint32_t>(in);for(uint32_t j=0;j<nu;j++){auto id=read<uint32_t>(in);Point p;for(auto&x:p)x=read<double>(in);frame.updates.push_back({id,p});}auto nq=read<uint32_t>(in);for(uint32_t j=0;j<nq;j++){Query q;for(auto&x:q.point)x=read<double>(in);q.radius=read<double>(in);frame.queries.push_back(q);}f.frames.push_back(std::move(frame));}return f;}

#if IMPLEMENTATION == 1
#include "../research/vendor/cpp-nanoflann/include/nanoflann.hpp"
struct Cloud{const std::vector<Point>&points;size_t kdtree_get_point_count()const{return points.size();}double kdtree_get_pt(size_t i,size_t d)const{return points[i][d];}template<class B>bool kdtree_get_bbox(B&)const{return false;}};
class Native{std::vector<Point> points;Cloud cloud;using Tree=nanoflann::KDTreeSingleIndexAdaptor<nanoflann::L2_Simple_Adaptor<double,Cloud>,Cloud,3,size_t>;std::unique_ptr<Tree> tree;bool dirty=false;mutable std::vector<nanoflann::ResultItem<size_t,double>> scratch;
 void rebuild(){tree=std::make_unique<Tree>(3,cloud,nanoflann::KDTreeSingleIndexAdaptorParams(16));}
 public:Native(const std::vector<Point>&p,double):points(p),cloud{points}{rebuild();}void update(const std::vector<std::pair<uint32_t,Point>>&u){for(auto&x:u)points[x.first]=x.second;dirty|=!u.empty();}void prepare(){if(dirty){rebuild();dirty=false;}}
 void query(const Point&p,double r,std::vector<uint64_t>&out)const{out.clear();nanoflann::SearchParameters params;params.sorted=false;tree->radiusSearch(p.data(),r*r+1e-9,scratch,params);for(auto&x:scratch)if(squared(points[x.first],p)<=r*r)out.push_back(x.first);}
};
#elif IMPLEMENTATION == 2
#include "pico_tree/array_traits.hpp"
#include "pico_tree/vector_traits.hpp"
#include "pico_tree/kd_tree.hpp"
class Native{std::vector<Point>points;using Tree=pico_tree::kd_tree<std::reference_wrapper<std::vector<Point>>>;std::unique_ptr<Tree>tree;bool dirty=false;mutable std::vector<Tree::neighbor_type> scratch;
 void rebuild(){tree=std::make_unique<Tree>(std::ref(points),pico_tree::max_leaf_size_t(16));}
 public:Native(const std::vector<Point>&p,double):points(p){rebuild();}void update(const std::vector<std::pair<uint32_t,Point>>&u){for(auto&x:u)points[x.first]=x.second;dirty|=!u.empty();}void prepare(){if(dirty){rebuild();dirty=false;}}
 void query(const Point&p,double r,std::vector<uint64_t>&out)const{out.clear();tree->search_radius(p,r*r+1e-9,scratch,false);for(auto&x:scratch)if(squared(points[x.index],p)<=r*r)out.push_back(x.index);}
};
#elif IMPLEMENTATION == 3
#include "../research/vendor/cpp-libkdtree/kdtree.h"
class Native{std::vector<Point>points;kdtree*tree=nullptr;bool dirty=false;
 void rebuild(){if(tree)kd_free(tree);tree=kd_create(3);for(size_t i=0;i<points.size();i++)kd_insert(tree,points[i].data(),reinterpret_cast<void*>(i+1));}
 public:Native(const std::vector<Point>&p,double):points(p){rebuild();}~Native(){kd_free(tree);}void update(const std::vector<std::pair<uint32_t,Point>>&u){for(auto&x:u)points[x.first]=x.second;dirty|=!u.empty();}void prepare(){if(dirty){rebuild();dirty=false;}}
 void query(const Point&p,double r,std::vector<uint64_t>&out)const{out.clear();auto result=kd_nearest_range(tree,p.data(),r+1e-9);for(;!kd_res_end(result);kd_res_next(result)){auto i=reinterpret_cast<uintptr_t>(kd_res_item_data(result))-1;if(squared(points[i],p)<=r*r)out.push_back(i);}kd_res_free(result);}
};
#elif IMPLEMENTATION == 4
#include "../research/vendor/cpp-kdtree-cpp/kdtree.hpp"
class Native{std::vector<Point>points;Kdtree::KdNodeVector nodes;std::unique_ptr<Kdtree::KdTree>tree;bool dirty=false;
 void rebuild(){nodes.clear();for(size_t i=0;i<points.size();i++)nodes.emplace_back(Kdtree::CoordPoint(points[i].begin(),points[i].end()),nullptr,int(i));tree=std::make_unique<Kdtree::KdTree>(&nodes);}
 public:Native(const std::vector<Point>&p,double):points(p){rebuild();}void update(const std::vector<std::pair<uint32_t,Point>>&u){for(auto&x:u)points[x.first]=x.second;dirty|=!u.empty();}void prepare(){if(dirty){rebuild();dirty=false;}}
 void query(const Point&p,double r,std::vector<uint64_t>&out)const{out.clear();Kdtree::KdNodeVector found;tree->range_nearest_neighbors(Kdtree::CoordPoint(p.begin(),p.end()),r+1e-9,&found);for(auto&x:found)if(squared(points[x.index],p)<=r*r)out.push_back(x.index);}
};
#elif IMPLEMENTATION == 5
#include "../research/vendor/cpp-KDTree/KDTree.hpp"
class Native{std::vector<Point>points;std::unique_ptr<KDTree>tree;bool dirty=false;
 void rebuild(){pointVec p;for(auto&x:points)p.emplace_back(x.begin(),x.end());tree=std::make_unique<KDTree>(p);}
 public:Native(const std::vector<Point>&p,double):points(p){rebuild();}void update(const std::vector<std::pair<uint32_t,Point>>&u){for(auto&x:u)points[x.first]=x.second;dirty|=!u.empty();}void prepare(){if(dirty){rebuild();dirty=false;}}
 void query(const Point&p,double r,std::vector<uint64_t>&out)const{out.clear();for(auto i:tree->neighborhood_indices(point_t(p.begin(),p.end()),r+1e-9))if(squared(points[i],p)<=r*r)out.push_back(i);}
};
#elif IMPLEMENTATION == 6
#include "../research/vendor/cpp-gishi-kdtree/kdtree.h"
struct GPoint:std::array<double,3>{static const int DIM=3;};
class Native{std::vector<Point>points;std::vector<GPoint>gp;std::unique_ptr<kdt::KDTree<GPoint>>tree;bool dirty=false;
 void rebuild(){gp.resize(points.size());for(size_t i=0;i<points.size();i++)std::copy(points[i].begin(),points[i].end(),gp[i].begin());tree=std::make_unique<kdt::KDTree<GPoint>>(gp);}
 public:Native(const std::vector<Point>&p,double):points(p){rebuild();}void update(const std::vector<std::pair<uint32_t,Point>>&u){for(auto&x:u)points[x.first]=x.second;dirty|=!u.empty();}void prepare(){if(dirty){rebuild();dirty=false;}}
 void query(const Point&p,double r,std::vector<uint64_t>&out)const{out.clear();GPoint q;std::copy(p.begin(),p.end(),q.begin());for(auto i:tree->radiusSearch(q,r+1e-9))if(squared(points[i],p)<=r*r)out.push_back(i);}
};
#elif IMPLEMENTATION == 7
#include "../research/variants/kdtree-plusplus/adapter.hpp"
#else
#error Set IMPLEMENTATION to a supported adapter
#endif

using Clock=std::chrono::steady_clock;
uint64_t elapsed(Clock::time_point t){return std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now()-t).count();}
struct Expected{bool all=false;size_t count=0;std::vector<uint32_t>ids;std::vector<uint64_t>bits;};
Expected expected_window(const std::vector<Point>&points,const Query&q,const Point&lo,const Point&hi){
 Expected e;Point far;for(size_t d=0;d<3;d++)far[d]=std::max(std::abs(q.point[d]-lo[d]),std::abs(q.point[d]-hi[d]));
 if(squared(far,Point{0,0,0})<=q.radius*q.radius){e.all=true;e.count=points.size();return e;}
 size_t words=(points.size()+63)/64;
 for(size_t i=0;i<points.size();i++)if(squared(points[i],q.point)<=q.radius*q.radius){e.count++;if(e.bits.empty()){e.ids.push_back(uint32_t(i));if(e.ids.size()>words*2){e.bits.resize(words,0);for(auto id:e.ids)e.bits[id/64]|=uint64_t(1)<<(id%64);e.ids.clear();}}else e.bits[i/64]|=uint64_t(1)<<(i%64);}
 return e;
}
void validate_window(const Expected&e,const std::vector<uint64_t>&out,std::vector<uint64_t>&seen,size_t n){
 if(out.size()!=e.count)throw std::runtime_error("result count mismatch");std::fill(seen.begin(),seen.end(),0);
 for(auto id:out){if(id>=n)throw std::runtime_error("ID range");auto mask=uint64_t(1)<<(id%64);if(seen[id/64]&mask)throw std::runtime_error("duplicate ID");seen[id/64]|=mask;if(!e.bits.empty()&&!(e.bits[id/64]&mask))throw std::runtime_error("unexpected ID");}
 for(auto id:e.ids)if(!(seen[id/64]&(uint64_t(1)<<(id%64))))throw std::runtime_error("missing ID");
}
int run_bounded(const Fixture&f,size_t samples){
 size_t window=std::getenv("RILL_OUTPUT_WINDOW")?std::stoul(std::getenv("RILL_OUTPUT_WINDOW")):64;if(window<1||window>64)throw std::runtime_error("output window admission");
 if(f.points.size()>100000)throw std::runtime_error("bounded population admission");for(auto&p:f.points)for(auto x:p)if(!std::isfinite(x)||std::abs(x)>1e6)throw std::runtime_error("bounded coordinate domain");
 for(auto&fr:f.frames){if(fr.queries.size()>20000)throw std::runtime_error("bounded query admission");for(auto&q:fr.queries){if(!(q.radius>=1e-100&&q.radius<=1e100))throw std::runtime_error("bounded radius domain");for(auto x:q.point)if(!std::isfinite(x)||std::abs(x)>1e6)throw std::runtime_error("bounded coordinate domain");}for(auto&u:fr.updates){if(u.first>=f.points.size())throw std::runtime_error("update ID range");for(auto x:u.second)if(!std::isfinite(x)||std::abs(x)>1e6)throw std::runtime_error("bounded coordinate domain");}}
 size_t query_count=0;for(auto&fr:f.frames)query_count+=fr.queries.size();size_t peak=0;std::vector<size_t>counts;uint64_t excluded_total=0;
 std::cout<<"{\"implementation\":\""<<IMPLEMENTATION_NAME<<"\",\"scenario\":\""<<f.name<<"\",\"points\":"<<f.points.size()<<",\"queries\":"<<query_count<<",\"correct\":true,\"materialization\":\"bounded reusable complete u64 result vectors; every ID independently verified\",\"timing_scope\":\"summed kernel service time with oracle pauses excluded; not uninterrupted tick latency\",\"samples\":[";
 for(size_t sample=0;sample<samples;sample++){
  auto reference=f.points;std::vector<uint64_t>seen((reference.size()+63)/64);auto t=Clock::now();Native index(f.points,f.cell_size);auto build=elapsed(t);uint64_t updates=0,queries=0;size_t results=0;
  for(auto&fr:f.frames){t=Clock::now();index.update(fr.updates);if(!fr.queries.empty())index.prepare();updates+=elapsed(t);for(auto&u:fr.updates)reference[u.first]=u.second;if(fr.queries.empty())continue;
   Point lo{INFINITY,INFINITY,INFINITY},hi{-INFINITY,-INFINITY,-INFINITY};for(auto&p:reference)for(size_t d=0;d<3;d++){lo[d]=std::min(lo[d],p[d]);hi[d]=std::max(hi[d],p[d]);}
   auto start=Clock::now();std::vector<std::vector<uint64_t>>outputs(window);uint64_t excluded=0;
   for(size_t begin=0;begin<fr.queries.size();begin+=window){size_t len=std::min(window,fr.queries.size()-begin);for(size_t i=0;i<len;i++){auto&q=fr.queries[begin+i];index.query(q.point,q.radius,outputs[i]);}
    auto pause=Clock::now();for(size_t i=0;i<len;i++){validate_window(expected_window(reference,fr.queries[begin+i],lo,hi),outputs[i],seen,reference.size());results+=outputs[i].size();if(!sample)counts.push_back(outputs[i].size());}size_t capacity=0;for(auto&out:outputs)capacity+=out.capacity()*8;peak=std::max(peak,capacity);excluded+=elapsed(pause);
   }queries+=elapsed(start)-excluded;excluded_total+=excluded;
  }
  if(sample)std::cout<<",";std::cout<<"{\"build_ns\":"<<build<<",\"update_ns\":"<<updates<<",\"query_ns\":"<<queries<<",\"total_ns\":"<<build+updates+queries<<",\"results\":"<<results<<"}";
 }
 std::sort(counts.begin(),counts.end());double mean=0;for(auto x:counts)mean+=x;mean/=std::max(size_t(1),counts.size());size_t p99=counts.empty()?0:counts[std::min(counts.size()-1,counts.size()*99/100)],maximum=counts.empty()?0:counts.back();
 std::cout<<"],\"output_window_queries\":"<<window<<",\"output_buffer_capacity_bytes\":"<<peak<<",\"excluded_validation_ns\":"<<excluded_total<<",\"matches_per_query\":{\"mean\":"<<mean<<",\"p99\":"<<p99<<",\"max\":"<<maximum<<"}}\n";return 0;
}

int main(int argc,char**argv){try{if(argc<2)return 2;auto f=load(argv[1]);size_t samples=argc>2?std::stoul(argv[2]):5;if(argc>3&&std::string(argv[3])=="bounded")return run_bounded(f,samples);auto points=f.points;std::vector<std::vector<uint64_t>>expected;
 for(auto&frame:f.frames){for(auto&u:frame.updates)points[u.first]=u.second;for(auto&q:frame.queries){std::vector<uint64_t>out;for(size_t i=0;i<points.size();i++)if(squared(points[i],q.point)<=q.radius*q.radius)out.push_back(i);expected.push_back(std::move(out));}}
 std::cout<<"{\"implementation\":\""<<IMPLEMENTATION_NAME<<"\",\"scenario\":\""<<f.name<<"\",\"points\":"<<points.size()<<",\"queries\":"<<expected.size()<<",\"correct\":true,\"samples\":[";
 for(size_t s=0;s<samples;s++){std::vector<std::vector<uint64_t>>out(expected.size());auto t=Clock::now();Native index(f.points,f.cell_size);auto build=elapsed(t);uint64_t updates=0,queries=0;size_t cursor=0;
  for(auto&frame:f.frames){t=Clock::now();index.update(frame.updates);if(!frame.queries.empty())index.prepare();updates+=elapsed(t);t=Clock::now();for(auto&q:frame.queries)index.query(q.point,q.radius,out[cursor++]);queries+=elapsed(t);}
  size_t results=0;for(size_t i=0;i<out.size();i++){std::sort(out[i].begin(),out[i].end());if(out[i]!=expected[i])throw std::runtime_error("membership mismatch at query "+std::to_string(i));results+=out[i].size();}
  if(s)std::cout<<",";std::cout<<"{\"build_ns\":"<<build<<",\"update_ns\":"<<updates<<",\"query_ns\":"<<queries<<",\"total_ns\":"<<build+updates+queries<<",\"results\":"<<results<<"}";
 }std::cout<<"]}\n";return 0;}catch(std::exception&e){std::cerr<<e.what()<<"\n";return 1;}}
