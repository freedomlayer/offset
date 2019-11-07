use app::common::PublicKey;
use app::report::NodeReport;

/// Find a friend's public key given his name
pub fn friend_public_key_by_name<'a>(
    node_report: &'a NodeReport,
    friend_name: &str,
) -> Option<&'a PublicKey> {
    // Search for the friend:
    for (friend_public_key, friend_report) in &node_report.funder_report.friends {
        if friend_report.name == friend_name {
            return Some(friend_public_key);
        }
    }
    None
}
