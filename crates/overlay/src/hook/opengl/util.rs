use scopeguard::defer;

pub fn restore_renderer_gl_state<R>(f: impl FnOnce() -> R) -> R {
    macro_rules! get_gl_int {
        ($name:ident = $expr:expr) => {
            let mut $name = 0;
            gl::GetIntegerv($expr, &mut $name);
        };
    }

    macro_rules! is_gl_enabled {
        ($name:ident = $expr:expr) => {
            let $name = gl::IsEnabled($expr) != 0;
        };
    }

    unsafe {
        // bindings
        get_gl_int!(last_active_texture = gl::ACTIVE_TEXTURE);
        get_gl_int!(last_program = gl::CURRENT_PROGRAM);
        get_gl_int!(last_texture = gl::TEXTURE_BINDING_2D);
        get_gl_int!(last_array_buffer = gl::ARRAY_BUFFER_BINDING);

        // vao
        get_gl_int!(last_vertex_array_object = gl::VERTEX_ARRAY_BINDING);

        // blending
        get_gl_int!(last_blend_src_rgb = gl::BLEND_SRC_RGB);
        get_gl_int!(last_blend_dst_rgb = gl::BLEND_DST_RGB);
        get_gl_int!(last_blend_src_alpha = gl::BLEND_SRC_ALPHA);
        get_gl_int!(last_blend_dst_alpha = gl::BLEND_DST_ALPHA);
        get_gl_int!(last_blend_equation_rgb = gl::BLEND_EQUATION_RGB);
        get_gl_int!(last_blend_equation_alpha = gl::BLEND_EQUATION_ALPHA);
        is_gl_enabled!(last_blend = gl::BLEND);
        is_gl_enabled!(last_cull_face = gl::CULL_FACE);
        is_gl_enabled!(last_depth_test = gl::DEPTH_TEST);
        is_gl_enabled!(last_stencil = gl::STENCIL_TEST);
        is_gl_enabled!(last_scissor_test = gl::SCISSOR_TEST);

        defer!({
            gl::ActiveTexture(last_active_texture as _);
            gl::UseProgram(last_program as _);
            gl::BindTexture(gl::TEXTURE_2D, last_texture as _);
            gl::BindBuffer(gl::ARRAY_BUFFER, last_array_buffer as _);

            gl::BindVertexArray(last_vertex_array_object as _);

            gl::BlendEquationSeparate(last_blend_equation_rgb as _, last_blend_equation_alpha as _);
            gl::BlendFuncSeparate(
                last_blend_src_rgb as _,
                last_blend_dst_rgb as _,
                last_blend_src_alpha as _,
                last_blend_dst_alpha as _,
            );
            if !last_blend {
                gl::Disable(gl::BLEND);
            }

            if last_cull_face {
                gl::Enable(gl::CULL_FACE);
            }

            if last_depth_test {
                gl::Enable(gl::DEPTH_TEST);
            }

            if last_stencil {
                gl::Enable(gl::STENCIL_TEST);
            }

            if last_scissor_test {
                gl::Enable(gl::SCISSOR_TEST);
            }
        });
        f()
    }
}
