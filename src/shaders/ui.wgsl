// Vertex shader

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
        // @location(2) full_frame: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
        // @location(1) full_frame: vec2<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    // out.full_frame = model.full_frame;
    out.clip_position = vec4<f32>(model.position, 1.0);
    return out;
}

struct UiRenderData {
    flagged: u32,
    transparency: f32,
    mouse_y: u32,
    mouse_x: u32,
    surface_height: u32,
    surface_width: u32,
    select_y: u32,
    select_x: u32,
    surface_start_x: u32,
    surface_start_y: u32,
    surface_end_x: u32,
    surface_end_y: u32,
    gs_r: f32,
    gs_g: f32,
    gs_b: f32,
    gs_sensitivity: f32,
    time: f32,
 }

 

const DisplayOverlays: u32 = 1;
const MouseOverWindow: u32 = 2;
const MouseDown: u32 = 4;
const WaitingForCrop: u32 = 8;
const OnlyAngles: u32 = 16;
const KeepBorders: u32 = 32;
const UseGreenScreen: u32 = 64;

// Fragment shader

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;
@group(0) @binding(2)
var overlay: texture_2d<f32>;

@group(0) @binding(3)
var<uniform> flags: UiRenderData;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = vec4(0.0, 0.0, 0.0, 0.0);

    color.a = color.a - (1.0 - flags.transparency);

    if ((flags.flagged & MouseOverWindow) != 0 || (flags.flagged & KeepBorders) != 0) && (flags.flagged & DisplayOverlays) != 0 {

        var resize_bar_size: u32 = 5;
        var icon_bar_size: u32 = resize_bar_size + 38;
        var corner_extend: u32 = 10;

        var is_corner: bool = false;

        if (flags.flagged & OnlyAngles) != 0 {
            if (u32(in.clip_position.y) < resize_bar_size + corner_extend || u32(in.clip_position.y) > flags.surface_height - (resize_bar_size + corner_extend)) && (u32(in.clip_position.x) < resize_bar_size + corner_extend || u32(in.clip_position.x) > flags.surface_width - (resize_bar_size + corner_extend)) {
                is_corner = true;
            }
        }

        if u32(in.clip_position.y) < icon_bar_size {
            if u32(in.clip_position.y) < resize_bar_size || u32(in.clip_position.x) < resize_bar_size || u32(in.clip_position.x) > flags.surface_width - resize_bar_size {
                if ((flags.flagged & OnlyAngles) != 0 && is_corner) || (flags.flagged & OnlyAngles) == 0 {
                    if (flags.flagged & MouseOverWindow) != 0 {
                        color.r = 0.66 / 2.0;
                        color.g = 1.0 / 2.0;
                        color.a = 0.9 ;
                    } else {
                        color.r = 0.66;
                        color.g = 1.0;
                        color.a = 0.9;
                    }
                } else {
                    if (flags.flagged & MouseOverWindow) != 0 {
                        color.a = 0.9;
                    }
                }
            } else {
                if (flags.flagged & MouseOverWindow) != 0 {
                    color.a = 0.9;
                }
            }
        }

        if u32(in.clip_position.x) < resize_bar_size && u32(in.clip_position.y) >= icon_bar_size {
            if ((flags.flagged & OnlyAngles) != 0 && is_corner) || (flags.flagged & OnlyAngles) == 0 {
                color.r = 0.66;
                color.g = 1.0;
                color.a = 0.9;
            }
        }


        if u32(in.clip_position.x) > flags.surface_width - resize_bar_size && u32(in.clip_position.y) >= icon_bar_size {
            if ((flags.flagged & OnlyAngles) != 0 && is_corner) || (flags.flagged & OnlyAngles) == 0 {
                color.r = 0.66;
                color.g = 1.0;
                color.a = 0.9;
            }
        }

        if u32(in.clip_position.y) > flags.surface_height - resize_bar_size {
            if ((flags.flagged & OnlyAngles) != 0 && is_corner) || (flags.flagged & OnlyAngles) == 0 {
                color.r = 0.66;
                color.g = 1.0;
                color.a = 0.9;
            }
        }
    }


    // This was for the hacked together crop button, but it's removed now
    //
    // if u32(in.clip_position.x) < 100 && u32(in.clip_position.y) < 100 {
    //     // crop button with mouse over before click
    //     if (u32(flags.mouse_x) < 100 && u32(flags.mouse_y) < 100) && (flags.flagged & WaitingForCrop) == 0 {
    //         color = vec4(1.0, 1.0, 1.0, 1.0);
    //     // crop button after click
    //     } else if (flags.flagged & WaitingForCrop) != 0 {
    //         color = vec4(0.3, 0.6, 0.9, 1.0);
    //     // other/unused 
    //     } else if (flags.flagged & DisplayOverlays) != 0 {
    //         // color = vec4(1.0, 1.0, 1.0, 0.5);
    //     }
    // }

    var in_cut_region = false;

    if (flags.flagged & MouseDown) != 0 {
        var min_x = min(flags.mouse_x, flags.select_x);
        var max_x = max(flags.mouse_x, flags.select_x);
        var min_y = min(flags.mouse_y, flags.select_y);
        var max_y = max(flags.mouse_y, flags.select_y);

        var color_temp = color;

        var sub_x: i32 = i32(min_x) - 5;
        var sub_y: i32 = i32(min_y) - 5;

        sub_x = max(sub_x, 0);
        sub_y = max(sub_y, 0);

        // shade everything dark because the slection started
        if (flags.flagged & WaitingForCrop) != 0 {
            color = vec4(0, 0, 0, 0.5);
        }
        
        if u32(in.clip_position.x) >= u32(sub_x) && u32(in.clip_position.x) <= max_x + 5 && u32(in.clip_position.y) >= u32(sub_y) && u32(in.clip_position.y) <= max_y + 5 {
            var mirror = textureSample(t_diffuse, s_diffuse, in.tex_coords);

            var m_red = 1.0;
            var m_green = 1.0;
            var m_blue = 1.0;

            if mirror.r > 0.5 {
                m_red = 0.0;
            }
            
            if mirror.b > 0.5 {
                m_blue = 0.0;
            }
            
            if mirror.g > 0.5 {
                m_green = 0.0;
            }

            color = vec4(m_red, m_green, m_blue, 1.0);

            in_cut_region = true;
        }


        if u32(in.clip_position.x) >= min_x && u32(in.clip_position.x) <= max_x && u32(in.clip_position.y) >= min_y && u32(in.clip_position.y) <= max_y {
            color = color_temp;

            color.a = 0.0;
        } 

        // displaying that crops outside of frame are snapping to dimensions of the frame
        if (flags.flagged & WaitingForCrop) != 0 {
            var border_size: u32 = 5;

            var vibrant_bound = vec4(0.0,
                1.0,
                0.24000001,
                0.9);

            // east
            if min_x < flags.surface_start_x {
                if u32(in.clip_position.y) >= min_y && u32(in.clip_position.y) <= max_y {
                    if u32(in.clip_position.x) >= flags.surface_start_x - border_size && u32(in.clip_position.x) < flags.surface_start_x && u32(in.clip_position.y) >= flags.surface_start_y - border_size && u32(in.clip_position.y) <= flags.surface_end_y + border_size {

                        color = vibrant_bound;
                    }
                }
            }

            // west
            if max_x > flags.surface_end_x {
                if u32(in.clip_position.y) >= min_y && u32(in.clip_position.y) <= max_y {
                    if u32(in.clip_position.x) >= flags.surface_end_x && u32(in.clip_position.x) <= flags.surface_end_x + border_size && u32(in.clip_position.y) >= flags.surface_start_y - border_size && u32(in.clip_position.y) < flags.surface_end_y + border_size {

                        color = vibrant_bound;
                    }
                }
            }

            // north
            if min_y < flags.surface_start_y {
                if u32(in.clip_position.x) >= min_x && u32(in.clip_position.x) <= max_x {
                    if u32(in.clip_position.y) >= flags.surface_start_y - border_size && u32(in.clip_position.y) < flags.surface_start_y && u32(in.clip_position.x) >= flags.surface_start_x - border_size && u32(in.clip_position.x) <= flags.surface_end_x + border_size {

                        color = vibrant_bound;
                    }
                }
            }

            // south
            if max_y > flags.surface_end_y {
                if u32(in.clip_position.x) >= min_x && u32(in.clip_position.x) <= max_x {
                    if u32(in.clip_position.y) >= flags.surface_end_y && u32(in.clip_position.y) <= flags.surface_end_y + border_size && u32(in.clip_position.x) >= flags.surface_start_x - border_size && u32(in.clip_position.x) < flags.surface_end_x + border_size {

                        color = vibrant_bound;
                    }
                }
            }
        }
    }


    if (flags.flagged & WaitingForCrop) != 0 && !in_cut_region {
   
            let x = in.tex_coords.x;
            let y = in.tex_coords.y;

            let zx = u32(f32(flags.surface_width) * x);
            let zy = u32(f32(flags.surface_height) * y);

            if (zx == flags.mouse_x || zy == flags.mouse_y) && (flags.flagged & MouseDown) == 0  {

                var mirror = textureSample(t_diffuse, s_diffuse, in.tex_coords);

                var m_red = 1.0;
                var m_green = 1.0;
                var m_blue = 1.0;

                if mirror.r > 0.5 {
                    m_red = 0.0;
                }
                
                if mirror.b > 0.5 {
                    m_blue = 0.0;
                }
                
                if mirror.g > 0.5 {
                    m_green = 0.0;
                }


                color = vec4(m_red, m_green, m_blue, 1.0);
            }



        var color2: vec4<f32> = textureSample(overlay, s_diffuse, in.tex_coords);

        if color2.r != 0.0 || color2.g != 0.0 || color2.b != 0.0 {
            color = color2;
        }
    }

    if // (flags.flagged & DisplayOverlays) != 0 && 
    
    !in_cut_region {

        var color2: vec4<f32> = textureSample(overlay, s_diffuse, in.tex_coords);

        if color2.r != 0.0 || color2.g != 0.0 || color2.b != 0.0 {
            color = color2;
        } else if color2.a != 0 {
            color = vec4(1.0, 1.0, 1.0, color2.a);
        }
    }








    return color;
}

 

 