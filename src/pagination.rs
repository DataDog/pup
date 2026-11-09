//! Shared pagination flags for paginated commands.
//!
//! Commands flatten [`CursorArgs`] or [`PageArgs`] into their clap args and
//! call `resolve` with a per-command spec to get validated, typed values.

use anyhow::{bail, Result};
use clap::Args;

/// Arg id of the public cursor flag (used to build continuation hints).
pub const CURSOR_ARG: &str = "pagination_cursor";
/// Arg id of the public page flag (used to build continuation hints).
pub const PAGE_ARG: &str = "pagination_page";

/// Whether the API numbers its pages from 0 or from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexBase {
    Zero,
    One,
}

impl IndexBase {
    fn value(self) -> u64 {
        match self {
            IndexBase::Zero => 0,
            IndexBase::One => 1,
        }
    }
}

/// Page size rules for one command. `None` default means "send nothing";
/// `None` max means no local cap.
#[derive(Clone, Copy, Debug)]
pub struct PageSizeSpec {
    pub default: Option<u64>,
    pub max: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
pub struct CursorSpec {
    pub size: PageSizeSpec,
}

#[derive(Clone, Copy, Debug)]
pub struct PageSpec {
    pub size: PageSizeSpec,
    pub base: IndexBase,
}

#[derive(Args, Clone, Debug, Default)]
pub struct CursorArgs {
    /// Cursor from a previous response's metadata.next_cursor
    #[arg(long = "cursor", id = CURSOR_ARG, aliases = ["page-cursor", "page-token"])]
    pub cursor: Option<String>,
    /// Results per page
    #[arg(long = "page-size", alias = "limit")]
    pub page_size: Option<u64>,
}

#[derive(Args, Clone, Debug, Default)]
pub struct PageArgs {
    /// Page number (0-based)
    #[arg(long = "page", id = PAGE_ARG)]
    pub page: Option<u64>,
    /// Results per page
    #[arg(long = "page-size", aliases = ["per-page", "limit"])]
    pub page_size: Option<u64>,
    /// Legacy: page number passed to the API unchanged
    #[arg(long = "page-number", hide = true, conflicts_with = PAGE_ARG)]
    pub page_number: Option<u64>,
    /// Legacy: result offset, converted to a page
    #[arg(long = "offset", hide = true, conflicts_with_all = [PAGE_ARG, "page_number"])]
    pub offset: Option<u64>,
}

fn resolve_size(given: Option<u64>, spec: &PageSizeSpec) -> Result<Option<u64>> {
    let size = given.or(spec.default);
    if let Some(n) = size {
        if n == 0 {
            bail!("--page-size must be at least 1");
        }
        if let Some(max) = spec.max {
            if n > max {
                bail!("--page-size must be between 1 and {max}, got {n}");
            }
        }
    }
    Ok(size)
}

fn convert<T: TryFrom<u64>>(name: &str, v: u64) -> Result<T> {
    T::try_from(v).map_err(|_| anyhow::anyhow!("{name} value {v} is too large"))
}

#[derive(Clone, Debug)]
pub struct ResolvedCursor {
    pub cursor: Option<String>,
    page_size: Option<u64>,
}

impl CursorArgs {
    pub fn resolve(&self, spec: &CursorSpec) -> Result<ResolvedCursor> {
        Ok(ResolvedCursor {
            cursor: self.cursor.clone(),
            page_size: resolve_size(self.page_size, &spec.size)?,
        })
    }
}

impl ResolvedCursor {
    pub fn page_size_opt<T: TryFrom<u64>>(&self) -> Result<Option<T>> {
        self.page_size
            .map(|n| convert("--page-size", n))
            .transpose()
    }

    /// Query params for raw HTTP calls; unset values are omitted.
    pub fn query_params(
        &self,
        cursor_key: &'static str,
        size_key: &'static str,
    ) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(c) = &self.cursor {
            out.push((cursor_key, c.clone()));
        }
        if let Some(n) = self.page_size {
            out.push((size_key, n.to_string()));
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedPage {
    page: u64,
    page_size: Option<u64>,
    base: IndexBase,
}

impl PageArgs {
    pub fn resolve(&self, spec: &PageSpec) -> Result<ResolvedPage> {
        let page_size = resolve_size(self.page_size, &spec.size)?;
        let page = if let Some(p) = self.page {
            p
        } else if let Some(n) = self.page_number {
            n.saturating_sub(spec.base.value())
        } else if let Some(o) = self.offset {
            match page_size {
                Some(s) if o % s == 0 => o / s,
                Some(s) => bail!(
                    "--offset {o} is not a multiple of --page-size {s}; use --page {} (starts at offset {}) instead",
                    o / s,
                    o / s * s
                ),
                None if o == 0 => 0,
                None => bail!("--page/--offset needs --page-size for this command"),
            }
        } else {
            0
        };
        if page > 0 && page_size.is_none() {
            bail!("--page/--offset needs --page-size for this command");
        }
        Ok(ResolvedPage {
            page,
            page_size,
            base: spec.base,
        })
    }
}

impl ResolvedPage {
    /// 0-based page index.
    pub fn page(&self) -> u64 {
        self.page
    }

    /// Page number in the API's own index base.
    pub fn api_page<T: TryFrom<u64>>(&self) -> Result<T> {
        convert("--page", self.page + self.base.value())
    }

    pub fn page_size<T: TryFrom<u64>>(&self) -> Result<T> {
        match self.page_size {
            Some(n) => convert("--page-size", n),
            None => bail!("--page-size is required for this command"),
        }
    }

    pub fn page_size_opt<T: TryFrom<u64>>(&self) -> Result<Option<T>> {
        self.page_size
            .map(|n| convert("--page-size", n))
            .transpose()
    }

    /// Result offset (page × page size).
    pub fn offset<T: TryFrom<u64>>(&self) -> Result<T> {
        let size = self.page_size.unwrap_or(0);
        convert("offset", self.page * size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct C {
        #[command(flatten)]
        p: CursorArgs,
    }
    #[derive(Parser)]
    struct P {
        #[command(flatten)]
        p: PageArgs,
    }

    const CS: CursorSpec = CursorSpec {
        size: PageSizeSpec {
            default: Some(50),
            max: Some(100),
        },
    };
    const NOSIZE: PageSizeSpec = PageSizeSpec {
        default: None,
        max: None,
    };
    const PS: PageSpec = PageSpec {
        size: PageSizeSpec {
            default: Some(25),
            max: Some(100),
        },
        base: IndexBase::Zero,
    };
    const PS1: PageSpec = PageSpec {
        size: PageSizeSpec {
            default: Some(10),
            max: None,
        },
        base: IndexBase::One,
    };

    fn c(args: &[&str]) -> CursorArgs {
        C::try_parse_from(std::iter::once("t").chain(args.iter().copied()))
            .unwrap()
            .p
    }
    fn p(args: &[&str]) -> Result<PageArgs, clap::Error> {
        P::try_parse_from(std::iter::once("t").chain(args.iter().copied())).map(|x| x.p)
    }

    #[test]
    fn cursor_aliases() {
        for f in ["--cursor", "--page-cursor", "--page-token"] {
            assert_eq!(c(&[f, "abc"]).cursor.as_deref(), Some("abc"));
        }
        assert_eq!(c(&["--limit", "7"]).page_size, Some(7));
        assert_eq!(c(&["--page-size", "7"]).page_size, Some(7));
    }

    #[test]
    fn page_aliases() {
        for f in ["--page-size", "--per-page", "--limit"] {
            assert_eq!(p(&[f, "5"]).unwrap().page_size, Some(5));
        }
    }

    #[test]
    fn defaults_and_none() {
        assert_eq!(
            c(&[]).resolve(&CS).unwrap().page_size_opt::<i64>().unwrap(),
            Some(50)
        );
        let spec = CursorSpec { size: NOSIZE };
        assert_eq!(
            c(&[])
                .resolve(&spec)
                .unwrap()
                .page_size_opt::<i64>()
                .unwrap(),
            None
        );
    }

    #[test]
    fn size_validation() {
        let e = c(&["--limit", "0"]).resolve(&CS).unwrap_err().to_string();
        assert!(e.contains("--page-size"));
        let e = c(&["--page-size", "101"])
            .resolve(&CS)
            .unwrap_err()
            .to_string();
        assert!(e.contains("between 1 and 100, got 101"));
        assert!(p(&["--per-page", "0"]).unwrap().resolve(&PS).is_err());
        assert!(p(&["--page-size", "100"]).unwrap().resolve(&PS).is_ok());
    }

    #[test]
    fn index_bases() {
        let r = p(&["--page", "0"]).unwrap().resolve(&PS1).unwrap();
        assert_eq!(r.api_page::<i64>().unwrap(), 1);
        let r = p(&["--page", "2"]).unwrap().resolve(&PS).unwrap();
        assert_eq!(r.api_page::<i64>().unwrap(), 2);
        // legacy --page-number passes through unchanged
        let r = p(&["--page-number", "1"]).unwrap().resolve(&PS1).unwrap();
        assert_eq!(r.page(), 0);
        assert_eq!(r.api_page::<i64>().unwrap(), 1);
        let r = p(&["--page-number", "3"]).unwrap().resolve(&PS).unwrap();
        assert_eq!(r.api_page::<i64>().unwrap(), 3);
        let r = p(&["--page-number", "0"]).unwrap().resolve(&PS1).unwrap();
        assert_eq!(r.page(), 0);
    }

    #[test]
    fn offset_conversion() {
        let r = p(&["--offset", "50", "--page-size", "25"])
            .unwrap()
            .resolve(&PS)
            .unwrap();
        assert_eq!(r.page(), 2);
        assert_eq!(r.offset::<usize>().unwrap(), 50);
        let e = p(&["--offset", "30", "--page-size", "25"])
            .unwrap()
            .resolve(&PS)
            .unwrap_err()
            .to_string();
        assert!(e.contains("--page 1"), "{e}");
        let r = p(&["--offset", "50"]).unwrap().resolve(&PS).unwrap();
        assert_eq!(r.page(), 2);
    }

    #[test]
    fn conflicts() {
        assert!(p(&["--offset", "5", "--page", "1"]).is_err());
        assert!(p(&["--offset", "5", "--page-number", "1"]).is_err());
        assert!(p(&["--page", "1", "--page-number", "1"]).is_err());
    }

    #[test]
    fn page_without_size_fails() {
        let spec = PageSpec {
            size: NOSIZE,
            base: IndexBase::Zero,
        };
        let e = p(&["--page", "2"])
            .unwrap()
            .resolve(&spec)
            .unwrap_err()
            .to_string();
        assert!(e.contains("needs --page-size"));
        assert!(p(&[]).unwrap().resolve(&spec).is_ok());
        assert!(p(&[])
            .unwrap()
            .resolve(&spec)
            .unwrap()
            .page_size::<i64>()
            .is_err());
    }

    #[test]
    fn getter_overflow() {
        let spec = CursorSpec { size: NOSIZE };
        let r = c(&["--page-size", "5000000000"]).resolve(&spec).unwrap();
        assert!(r.page_size_opt::<i32>().is_err());
        assert!(r.page_size_opt::<i64>().is_ok());
    }

    #[test]
    fn query_params_output() {
        let r = c(&["--cursor", "x", "--page-size", "9"])
            .resolve(&CS)
            .unwrap();
        assert_eq!(
            r.query_params("page[cursor]", "page[limit]"),
            vec![
                ("page[cursor]", "x".to_string()),
                ("page[limit]", "9".to_string())
            ]
        );
        let r = c(&[]).resolve(&CursorSpec { size: NOSIZE }).unwrap();
        assert!(r.query_params("a", "b").is_empty());
    }
}
