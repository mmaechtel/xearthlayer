//! XEarthLayer CLI - Command-line interface
//!
//! This binary provides a command-line interface to the XEarthLayer library.

fn main() {
    // Call the library function - demonstrating library → CLI architecture
    let message = xearthlayer::greeting();
    println!("{}", message);
}
