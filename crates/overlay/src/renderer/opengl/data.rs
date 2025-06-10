use scopeguard::defer;

pub fn with_renderer_gl_data<R>(f: impl FnOnce() -> R) -> R {
    macro_rules! get_gl_int {
        ($name:ident = $expr:expr) => {
            let mut $name = 0;
            gl::GetIntegerv($expr, &mut $name);
            let $name = $name as u32;
        };
    }

    macro_rules! is_gl_enabled {
        ($name:ident = $expr:expr) => {
            let $name = gl::IsEnabled($expr) == gl::TRUE;
        };
    }

    unsafe {
        // viewport
        let mut last_viewport = [0_i32; 4];
        gl::GetIntegerv(gl::VIEWPORT, last_viewport.as_mut_ptr());

        // bindings, mode
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
            // https://github.com/ocornut/imgui/issues/6220
            if last_program == 0 || gl::IsProgram(last_program) != 0 {
                gl::UseProgram(last_program);
            }

            gl::BindTexture(gl::TEXTURE_2D, last_texture);

            if last_active_texture != gl::TEXTURE0 {
                gl::ActiveTexture(last_active_texture);
            }

            gl::BindBuffer(gl::ARRAY_BUFFER, last_array_buffer);

            gl::BindVertexArray(last_vertex_array_object);

            gl::BlendEquationSeparate(last_blend_equation_rgb, last_blend_equation_alpha);
            gl::BlendFuncSeparate(
                last_blend_src_rgb,
                last_blend_dst_rgb,
                last_blend_src_alpha,
                last_blend_dst_alpha,
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

            if last_viewport[2] != 0 || last_viewport[3] != 0 {
                gl::Viewport(
                    last_viewport[0],
                    last_viewport[1],
                    last_viewport[2],
                    last_viewport[3],
                );
            }
        });
        f()
    }
}
