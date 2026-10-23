struct View { vp:mat4x4<f32>, viewport:vec4<f32> }
@group(0) @binding(0) var<uniform> view:View;
struct Out { @builtin(position) position:vec4<f32>, @location(0) color:vec4<f32> }
@vertex fn vs(@builtin(vertex_index) vertex:u32,@location(0) start:vec3<f32>,@location(1) end:vec3<f32>,@location(2) color:vec4<f32>,@location(3) width:f32)->Out {
    var out:Out;out.color=color;
    var a=view.vp*vec4(start,1.);var b=view.vp*vec4(end,1.);
    if (a.z<=0. && b.z<=0.) || (a.w<=0. && b.w<=0.) {out.position=vec4(2.,2.,1.,1.);return out;}
    if a.z<0. {a=mix(a,b,-a.z/(b.z-a.z));}
    if b.z<0. {b=mix(b,a,-b.z/(a.z-b.z));}
    if a.w<=1e-8 || b.w<=1e-8 {out.position=vec4(2.,2.,1.,1.);return out;}
    let delta=(b.xy/b.w-a.xy/a.w)*view.viewport.xy;
    if length(delta)<1e-5 {out.position=vec4(2.,2.,1.,1.);return out;}
    let normal=normalize(vec2(-delta.y,delta.x));
    let corners=array<vec2<f32>,6>(vec2(0.,-1.),vec2(1.,-1.),vec2(1.,1.),vec2(0.,-1.),vec2(1.,1.),vec2(0.,1.));
    let corner=corners[vertex];var clip=mix(a,b,corner.x);
    clip.x+=normal.x*corner.y*width*view.viewport.z/view.viewport.x*clip.w;
    clip.y+=normal.y*corner.y*width*view.viewport.z/view.viewport.y*clip.w;
    out.position=clip;return out;
}
@fragment fn fs(in:Out)->@location(0) vec4<f32> {return in.color;}
