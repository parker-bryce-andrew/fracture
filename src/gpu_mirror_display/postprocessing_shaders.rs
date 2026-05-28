use crate::gpu_mirror_display::state::Application;

use super::pipeline_definitions::define_primary_pipeline;
use naga::{
    Module,
    valid::{Capabilities, ModuleInfo, ValidationFlags},
};
use wgpu::{
    CompilationInfo, CompilationMessageType, Device, PipelineLayout, ShaderModule,
    ShaderRuntimeChecks,
};
use wgsl_formatter::FormattingOptions;

#[derive(Debug)]
pub struct PassedValidation {
    pub device_module: ShaderModule,
    #[allow(unused)]
    pub source: Module,
    #[allow(unused)]
    pub module_information: ModuleInfo,
    pub formatted: String,
}

impl PassedValidation {
    pub fn extract_postprocessor(&self) -> String {
        let (_, method) = self
            .formatted
            .split_once("// POSTPROCESSOR BELOW THIS LINE")
            .unwrap();

        method.into()
    }
}

pub static DEFAULT_POSTPROCESSOR: &'static str = "fn postprocess(in_color: vec4<f32>, in: VertexOutput, data: UiRenderData) -> vec4<f32> {\n\tvar color = in_color;\n\t// You can edit the shader here.\n\n\t// Invert colors\n\t// color = vec4(1.0 - color.r, 1.0 - color.g, 1.0 - color.b, color.a);\n\t// Uncomment the above line ^ to invert the colors.\n\n\treturn color;\n}";

#[derive(Debug, Clone)]
pub enum PostprocessingErrors {
    #[allow(unused)]
    ParseErr(naga::front::wgsl::ParseError),
    #[allow(unused)]
    ValidationErr(naga::WithSpan<naga::valid::ValidationError>),
    #[allow(unused)]
    CompilationErr(CompilationInfo),
}

pub async fn define_postprocessing_mirror_shader(
    device: &Device,
    postprocessor: Option<&str>,
) -> Result<PassedValidation, PostprocessingErrors> {
    let shader = include_str!("../shaders/mirror.wgsl").to_string();

    let mut shader = shader.replace(
        "// INJECT HERE 1",
        r#"
        color = postprocess(color, in, flags);
    "#,
    );

    if let Some(postprocessor) = postprocessor {
        shader = shader.replace("// INJECT HERE 2", postprocessor);
    } else {
        shader = shader.replace("// INJECT HERE 2", &DEFAULT_POSTPROCESSOR);
    }

    let shader = shader;

    let naga = wgpu::naga::front::wgsl::parse_str(&shader);

    if let Ok(testing) = naga {
        let mut validator =
            naga::valid::Validator::new(ValidationFlags::all(), Capabilities::all());

        let val_results = validator.validate(&testing);

        if let Ok(results) = val_results {
            let formatted = wgsl_formatter::format_str(&shader, &FormattingOptions::default());

            let shader = unsafe {
                device.create_shader_module_trusted(
                    wgpu::ShaderModuleDescriptor {
                        label: Some("Postprocessing Mirror Shader"),
                        source: wgpu::ShaderSource::Wgsl(shader.into()),
                    },
                    ShaderRuntimeChecks::checked(),
                )
            };

            let result = shader.get_compilation_info().await;

            if !result
                .messages
                .iter()
                .any(|a| a.message_type == CompilationMessageType::Error)
            {
                /*
                    // This is another way to format the entire shader.

                    let data =
                        naga::back::wgsl::write_string(&testing, &results, WriterFlags::all()).unwrap();

                    let data = data.split("\n");

                    for line in data {
                        println!("{line}");
                    }
                */

                let temp = PassedValidation {
                    source: testing,
                    module_information: results,
                    formatted,
                    device_module: shader,
                };

                Ok(temp)
            } else {
                Err(PostprocessingErrors::CompilationErr(result))
            }
        } else {
            Err(PostprocessingErrors::ValidationErr(
                val_results.unwrap_err(),
            ))
        }
    } else {
        Err(PostprocessingErrors::ParseErr(naga.unwrap_err()))
    }
}

pub fn if_shader_compilation_requested(
    app: &mut Application,
    rt: &tokio::runtime::Runtime,
    render_pipeline_layout: &PipelineLayout,
) {
    if app.configuration.active.gpu_requested_compile {
        if let Some(v) = &mut app.configuration.active.postprocessor {
            let new_shader = v.submitted_postprocessor.clone();

            let new_shader = if let Some(v) = &new_shader {
                let temp: Option<&str> = Some(&v);

                temp
            } else {
                None
            };

            let result = rt.block_on(define_postprocessing_mirror_shader(
                &app.systems.wgpu.device,
                new_shader.clone(),
            ));

            let mut return_value = v.clone();

            let module = if let Ok(good_compilation) = result {
                let auto_formatted = good_compilation.extract_postprocessor();

                return_value.editing_postprocessor = auto_formatted;
                return_value.last_errors = None;

                good_compilation.device_module
            } else {
                return_value.last_errors = Some(result.unwrap_err());

                let default = rt
                    .block_on(define_postprocessing_mirror_shader(
                        &app.systems.wgpu.device,
                        None,
                    ))
                    .unwrap();

                return_value.submitted_postprocessor = Some(default.extract_postprocessor());

                default.device_module
            };

            let pipeline = define_primary_pipeline(
                &app.systems.wgpu.device,
                &module,
                &render_pipeline_layout,
                &app.systems.wgpu.config.format,
            );

            app.mirror
                .render
                .mirror_rendering
                .mirror_output_rendering_pipeline = pipeline;

            *v = return_value;
        } else {
            let default = rt
                .block_on(define_postprocessing_mirror_shader(
                    &app.systems.wgpu.device,
                    None,
                ))
                .unwrap();

            let pipeline = define_primary_pipeline(
                &app.systems.wgpu.device,
                &default.device_module,
                &render_pipeline_layout,
                &app.systems.wgpu.config.format,
            );

            app.mirror
                .render
                .mirror_rendering
                .mirror_output_rendering_pipeline = pipeline;
        }

        app.configuration.active.gpu_requested_compile = false;

        app.external
            .channels
            .gpu_sender_request
            .send(app.configuration.active.clone())
            .unwrap();
    }
}
