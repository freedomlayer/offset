use std::path::PathBuf;
use backtrace::{self, Symbol};


#[derive(Clone, Debug)]
pub struct CallerInfo {
    /// The symbol name of the caller (Demangled):
    pub name: String,
    /// The file that contains the callsite:
    pub filename: PathBuf,
    /// Line number:
    pub lineno: u32,
}

fn symbol_to_caller_info(symbol: &Symbol) -> Option<CallerInfo> {

    let name = format!("{}", symbol.name()?);
    let filename = symbol.filename()?.to_path_buf();
    let lineno = symbol.lineno()?;

    Some(CallerInfo {
        name,
        filename,
        lineno,
    })
}

/// Get information about the caller, `level` levels above a frame that satisfies some predicate
/// `pred`.
pub fn get_caller_info<F>(mut level: usize, pred: F) -> Option<CallerInfo> 
where
    F: Fn(&CallerInfo) -> bool
{

    // Have we already found our function in the stack trace?
    let mut pred_found = false;
    let mut opt_caller_info = None;

    backtrace::trace(|frame| {
        let ip = frame.ip();

        let mut opt_cur_caller_info = None;
        backtrace::resolve(ip, |symbol| {
            opt_cur_caller_info = symbol_to_caller_info(&symbol);
        });

        let cur_caller_info = match opt_cur_caller_info {
            Some(cur_caller_info) => cur_caller_info,
            None => {
                opt_caller_info = None;
                return false;
            },
        };

        pred_found |= pred(&cur_caller_info);
        if !pred_found {
            // Move on to the next frame:
            return true;
        }

        // Wanted frame was already found:
        if level > 0 {
            level = level.saturating_sub(1);
            // Move on to the next frame:
            return true;
        }
        // We got to the interesting frame:

        opt_caller_info = Some(cur_caller_info); 

        false // Stop iterating
    });

    opt_caller_info
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_caller_info() {
        // Be careful: Adding new lines inside this test might break it, because 
        // line numbers are calculated:
        let cur_lineno = line!();
        let caller_info = get_caller_info(0, |caller_info| caller_info.name.contains("test_get_caller_info")).unwrap();
        assert!(caller_info.name.contains("test_get_caller_info"));
        assert_eq!(caller_info.lineno, cur_lineno + 1);
    }
}
