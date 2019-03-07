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

fn symbol_to_caller_info(symbol: &Symbol) -> CallerInfo {

    let name = format!("{}", symbol.name().unwrap());
    let filename = symbol.filename().unwrap().to_path_buf();
    let lineno = symbol.lineno().unwrap();

    CallerInfo {
        name,
        filename,
        lineno,
    }
}

/// Get information about the caller, `level` levels above a frame that satisfies some predicate
/// `pred`.
pub fn get_caller_info<F>(mut level: usize, pred: F) -> Option<CallerInfo> 
where
    F: Fn(&CallerInfo) -> bool
{

    // Have we already found our function in the stack trace?
    let mut pred_found = false;
    let mut output = None;

    backtrace::trace(|frame| {
        let ip = frame.ip();
        if !pred_found {
            backtrace::resolve(ip, |symbol| {
                // Check if this frame represents the current function:
                // let name = format!("{}", symbol.name().unwrap());
                let caller_info = symbol_to_caller_info(&symbol);
                if pred(&caller_info) {
                    pred_found = true;
                }
            });
            return true; // Move on to next frame
        }

        // self frame was already found:

        if level > 0 {
            level = level.saturating_sub(1);
            // Move on to the next frame:
            return true;
        }

        // We got to the interesting frame:

        backtrace::resolve(ip, |symbol| {
            // The previous frame was this function.
            // Get required info and return:
            // let name = format!("{}", symbol.name().unwrap());
            // let name = symbol.name().unwrap().as_str().unwrap().to_owned();

            output = Some(symbol_to_caller_info(&symbol));
        });

        false // Stop iterating
    });
    output
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_caller_info() {
        // Be careful: Adding new lines inside this test might break it, because 
        // line numbers are calculated:
        let cur_lineno = line!();
        let caller_info = get_caller_info(0, |caller_info| caller_info.name.contains("get_caller_info")).unwrap();
        assert!(caller_info.name.contains("test_get_caller_info"));
        assert_eq!(caller_info.lineno, cur_lineno + 1);
    }
}
