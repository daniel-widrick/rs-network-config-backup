use chrono::{DateTime, Utc};
use serde::Deserialize;
use ssh2::Session;
use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::{fmt, io};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HostRecord {
    name: String,
    address: String,
    username: String,
    password: String,
}

#[derive(Debug)]
enum BackupError {
    IOError(io::Error),
    SSHError(ssh2::Error),
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackupError::IOError(io_error) => write!(f, "{}", io_error),
            BackupError::SSHError(ssh_error) => write!(f, "{}", ssh_error),
        }
    }
}
impl From<io::Error> for BackupError {
    fn from(error: io::Error) -> Self {
        BackupError::IOError(error)
    }
}
impl From<ssh2::Error> for BackupError {
    fn from(error: ssh2::Error) -> Self {
        BackupError::SSHError(error)
    }
}

fn main() {
    println!("Hello, world!");
    //Load hosts from csv file
    let config_file_name = "hosts.csv";
    let config_file = File::open(config_file_name).unwrap();
    let mut config_reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(config_file);

    for result in config_reader.deserialize() {
        let record: HostRecord = result.unwrap();
        match backup_host(&record) {
            Ok(..) => println!("{} Backed up", record.name),
            Err(error) => println!("Backup Failed: {} :: {}", record.name, error),
        };
    }
}

fn backup_host(host_record: &HostRecord) -> Result<(), BackupError> {
    let backup_file_name = make_backup_file_name(&host_record.name);

    //Establish SSH Connection
    let tcp = TcpStream::connect(&host_record.address)?;
    let mut sess = Session::new()?;
    sess.set_tcp_stream(tcp);
    sess.handshake()?;
    //Authenticate
    sess.userauth_password(&host_record.username, &host_record.password)?;
    //Run backup
    let backup_cmd = format!(
        "/system/backup/save name={} dont-encrypt=yes",
        backup_file_name
    );
    let mut channel = sess.channel_session()?;
    channel.exec(&backup_cmd)?;

    return fetch_backup(&backup_file_name, &sess);
}
fn make_backup_file_name(hostname: &str) -> String {
    let now: DateTime<Utc> = Utc::now();
    let date_stamp = format!("{}", now.format("%Y%m%d-%H%M"));
    return format!("{}_{}.backup", hostname, date_stamp);
}
fn fetch_backup(filename: &str, sess: &Session) -> Result<(), BackupError> {
    let (mut remote_file, _) = sess.scp_recv(Path::new(filename))?;
    let mut contents: Vec<u8> = Vec::new();
    remote_file.read_to_end(&mut contents)?;
    remote_file.send_eof()?;
    remote_file.wait_eof()?;
    remote_file.close()?;
    remote_file.wait_close()?;

    std::fs::create_dir_all("backups")?;
    let file_path = format!("{}/{}", "backups", filename);
    let mut file = File::create(file_path)?;
    file.write_all(contents.as_slice())?;
    return Ok(());
}
