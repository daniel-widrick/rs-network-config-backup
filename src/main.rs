use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use chrono::{DateTime, Utc};
use ssh2::Session;

fn main() {
    println!("Hello, world!");

    //Get date for filename
    let now:DateTime<Utc> = Utc::now();
    let date_stamp = format!("{}", now.format("%Y%m%d-%H%M"));
    let mut password = File::open("password").unwrap();
    let mut password_text : Vec<u8> = Vec::new();
    password.read_to_end(&mut password_text).unwrap();

    let tcp = TcpStream::connect("192.168.67.1:22").unwrap();
    let mut sess = Session::new().unwrap();
    sess.set_tcp_stream(tcp);
    sess.handshake().unwrap();
    sess.userauth_password("admin",std::str::from_utf8(&password_text).unwrap()).unwrap();

    let mut channel = sess.channel_session().unwrap();
    let filename = format!("102-seymour-rb4011-{}.backup",date_stamp);
    let backup_cmd = format!("/system/backup/save name={} dont-encrypt=yes",filename);
    channel.exec(&backup_cmd).unwrap();

    let (mut remote_file,stat) = sess.scp_recv(Path::new(&filename)).unwrap();
    println!("File Size: {} :: {}",filename,stat.size());
    let mut contents = Vec::new();
    remote_file.read_to_end(&mut contents).unwrap();
    remote_file.send_eof();
    remote_file.wait_eof();
    remote_file.close();
    remote_file.wait_close().unwrap();

    let mut file = File::create(filename).unwrap();
    file.write_all(contents.as_slice()).unwrap();

}
