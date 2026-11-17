//! A Home's control socket: one Home names one, whether or not its folder was made yet, and however its path is spelled.

use octoii::home::Home;

#[cfg(unix)]
#[test]
fn a_home_names_one_socket_before_and_after_it_is_made_and_through_a_link() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path().join("home"));
    let before = home.socket().expect("its socket");
    home.prepare().expect("a Home");
    assert_eq!(home.socket().expect("its socket"), before, "made since");
    let link = folder.path().join("link");
    std::os::unix::fs::symlink(folder.path(), &link).expect("a link");
    assert_eq!(Home::at(link.join("home")).socket().expect("its socket"), before, "through a link");
}
