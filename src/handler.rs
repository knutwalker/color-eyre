// #[cfg(feature = "capture-backtrace")]
// use crate::config::installed_hook;
#[cfg(feature = "capture-backtrace")]
use crate::config::BacktraceFormatter;
#[cfg(any(feature = "capture-backtrace", feature = "capture-spantrace"))]
use crate::writers::EnvSection;
use crate::{section::help::HelpInfo, writers::WriterExt, Handler};
#[cfg(feature = "capture-backtrace")]
use backtrace::Backtrace;
use indenter::indented;
#[cfg(feature = "capture-backtrace")]
use indenter::Format;
use owo_colors::OwoColorize;
use std::fmt::Write;
#[cfg(feature = "capture-spantrace")]
use tracing_error::{ExtractSpanTrace, SpanTrace};

impl std::fmt::Debug for Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("redacted")
    }
}

impl Handler {
    /// Return a reference to the captured `Backtrace` type
    #[cfg(feature = "capture-backtrace")]
    #[cfg_attr(docsrs, doc(cfg(feature = "capture-backtrace")))]
    pub fn backtrace(&self) -> Option<&Backtrace> {
        self.backtrace.as_ref()
    }

    /// Return a reference to the captured `SpanTrace` type
    #[cfg(feature = "capture-spantrace")]
    #[cfg_attr(docsrs, doc(cfg(feature = "capture-spantrace")))]
    pub fn span_trace(&self) -> Option<&SpanTrace> {
        self.span_trace.as_ref()
    }

    #[cfg(feature = "capture-backtrace")]
    pub(crate) fn format_backtrace<'a>(
        &'a self,
        trace: &'a backtrace::Backtrace,
    ) -> BacktraceFormatter<'a> {
        BacktraceFormatter {
            filters: &self.filters,
            inner: trace,
        }
    }
}

impl eyre::EyreHandler for Handler {
    fn debug(
        &self,
        error: &(dyn std::error::Error + 'static),
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        if f.alternate() {
            return core::fmt::Debug::fmt(error, f);
        }

        #[cfg(feature = "capture-spantrace")]
        let errors = || {
            eyre::Chain::new(error)
                .filter(|e| e.span_trace().is_none())
                .enumerate()
        };

        #[cfg(not(feature = "capture-spantrace"))]
        let errors = || eyre::Chain::new(error).enumerate();

        for (n, error) in errors() {
            writeln!(f)?;
            write!(indented(f).ind(n), "{}", error.bright_red())?;
        }

        let mut separated = f.header("\n\n");

        #[cfg(feature = "track-caller")]
        write!(
            separated.ready(),
            "{}",
            crate::SectionExt::header(crate::fmt::LocationSection(self.location), "Location:")
        )?;

        for section in self
            .sections
            .iter()
            .filter(|s| matches!(s, HelpInfo::Error(_)))
        {
            write!(separated.ready(), "{}", section)?;
        }

        for section in self
            .sections
            .iter()
            .filter(|s| matches!(s, HelpInfo::Custom(_)))
        {
            write!(separated.ready(), "{}", section)?;
        }

        #[cfg(feature = "capture-spantrace")]
        let span_trace = self
            .span_trace
            .as_ref()
            .or_else(|| get_deepest_spantrace(error));

        #[cfg(feature = "capture-spantrace")]
        {
            if let Some(span_trace) = span_trace {
                write!(
                    &mut separated.ready(),
                    "{}",
                    crate::writers::FormattedSpanTrace(span_trace)
                )?;
            }
        }

        #[cfg(feature = "capture-backtrace")]
        {
            if let Some(backtrace) = self.backtrace.as_ref() {
                let fmted_bt = self.format_backtrace(&backtrace);

                write!(
                    indented(&mut separated.ready())
                        .with_format(Format::Uniform { indentation: "  " }),
                    "{}",
                    fmted_bt
                )?;
            }
        }

        let f = separated.ready();
        let mut h = f.header("\n");
        let mut f = h.in_progress();

        for section in self
            .sections
            .iter()
            .filter(|s| !matches!(s, HelpInfo::Custom(_) | HelpInfo::Error(_)))
        {
            write!(&mut f, "{}", section)?;
            f = h.ready();
        }

        #[cfg(any(feature = "capture-backtrace", feature = "capture-spantrace"))]
        if self.display_env_section {
            let env_section = EnvSection {
                #[cfg(feature = "capture-backtrace")]
                bt_captured: &self.backtrace.is_some(),
                #[cfg(feature = "capture-spantrace")]
                span_trace,
            };

            write!(&mut separated.ready(), "{}", env_section)?;
        }

        #[cfg(feature = "issue-url")]
        if self.issue_url.is_some() && (*self.issue_filter)(crate::ErrorKind::Recoverable(error)) {
            let url = self.issue_url.as_ref().unwrap();
            let mut payload = String::from("Error: ");
            for (n, error) in errors() {
                writeln!(&mut payload)?;
                write!(indented(&mut payload).ind(n), "{}", error)?;
            }

            let issue_section = crate::section::github::IssueSection::new(url, &payload)
                .with_metadata(&**self.issue_metadata);

            #[cfg(feature = "capture-backtrace")]
            let issue_section = issue_section.with_backtrace(self.backtrace.as_ref());

            #[cfg(feature = "capture-spantrace")]
            let issue_section = issue_section.with_span_trace(span_trace);

            write!(&mut separated.ready(), "{}", issue_section)?;
        }

        Ok(())
    }

    #[cfg(feature = "track-caller")]
    fn track_caller(&mut self, location: &'static std::panic::Location<'static>) {
        self.location = Some(location);
    }
}

#[cfg(feature = "capture-spantrace")]
pub(crate) fn get_deepest_spantrace<'a>(
    error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a SpanTrace> {
    eyre::Chain::new(error)
        .rev()
        .flat_map(|error| error.span_trace())
        .next()
}
