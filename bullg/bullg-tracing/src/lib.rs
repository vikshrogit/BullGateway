use anyhow::Result;
use tracing_subscriber::{ prelude::*, Registry };
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt;
use tracing_opentelemetry::OpenTelemetryLayer;
use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::{ trace as sdktrace, Resource };
use opentelemetry_stdout::SpanExporter;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_otlp::Protocol;
use std::borrow::Cow;

/// Initialize tracing + OpenTelemetry tracer provider.
pub fn init(service_name: &str, otlp_endpoint: Option<&str>, logging_mode: &str) -> Result<()> {
    // Build resource
    let resource = Resource::builder()
        .with_service_name(Cow::Owned(service_name.to_string()))
        .with_attributes(
            vec![
                KeyValue::new("service.version", "0.1.0"),
                KeyValue::new("deployment.environment", "development")
            ]
        )
        .build();

    // Choose exporter
    let tracer_provider = if let Some(endpoint) = otlp_endpoint {
        let exporter = opentelemetry_otlp::SpanExporter
            ::builder()
            .with_http()
            .with_protocol(Protocol::HttpBinary)
            .with_endpoint(endpoint)
            .build()?;

        sdktrace::SdkTracerProvider
            ::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .build()
    } else {
        sdktrace::SdkTracerProvider
            ::builder()
            .with_simple_exporter(SpanExporter::default())
            .with_resource(resource)
            .build()
    };

    // Create tracer
    let tracer = tracer_provider.tracer(Cow::Owned(service_name.to_string()));

    // Build OTEL layer
    let otel_layer = OpenTelemetryLayer::new(tracer);

    let filter = EnvFilter::try_new(logging_mode).unwrap_or_else(|_| EnvFilter::new("info"));
    //tracing_subscriber::fmt().with_env_filter(filter).init();

    // Logging modes (boxed trait objects so types unify)
    let fmt_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = {
        match logging_mode {
            "json" =>
                Box::new(
                    fmt
                        ::layer()
                        .json()
                        .with_thread_ids(true)
                        .with_thread_names(true)
                        .with_filter(filter)
                ),
            "pretty" => Box::new(fmt::layer().pretty().with_filter(filter)),
            _ => Box::new(fmt::layer().with_filter(filter)),
        }
    };

    // Combine subscriber
    let subscriber = Registry::default().with(fmt_layer).with(otel_layer);

    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}
