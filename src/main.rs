use chrono::{DateTime, Utc};
use serde::Deserialize;
use ssh2::{Error, ErrorCode, ExtendedData, MethodType, Session, TraceFlags};
use std::fs::File;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::{fmt, io, thread, time};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HostRecord {
    name: String,
    address: String,
    username: String,
    password: String,
    method: String
}
#[derive(Debug)]
struct UnknownBackupMethod{
    details: String
}
impl UnknownBackupMethod {
    fn new(msg: &str) -> UnknownBackupMethod {
        UnknownBackupMethod{details: msg.to_string()}
    }
}
impl fmt::Display for UnknownBackupMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,"Unknown Backup type detected")
    }
}
impl std::error::Error for UnknownBackupMethod {
    fn description(&self) -> &str {
        &self.details
    }
}


#[derive(Debug)]
enum BackupError {
    IOError(io::Error),
    SSHError(ssh2::Error),
    UnknownBackupMethod(UnknownBackupMethod),
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackupError::IOError(io_error) => write!(f, "{}", io_error),
            BackupError::SSHError(ssh_error) => write!(f, "{}", ssh_error),
            BackupError::UnknownBackupMethod(ube) => write!(f, "{}", ube.details)
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
impl From<UnknownBackupMethod> for BackupError {
    fn from(error: UnknownBackupMethod) -> Self { BackupError::UnknownBackupMethod(error) }
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

fn backup_host(host_record : &HostRecord) -> Result<(), BackupError> {
    if host_record.name.starts_with('#') {
        return Ok(()); //Skip comments
    }
    match host_record.method.as_ref() {
        "Mikrotik-Binary" => return backup_mikrotik_binary_host(host_record),
        "Mikrotik-Export" => return backup_mikrotik_export_host(host_record),
        "Cisco-Export" => return backup_cisco_export_host(host_record),
        "HP-Export" => return backup_hp_export_host(host_record),
        _ => return Err(UnknownBackupMethod::new(format!("Unknown Method: {}",host_record.method).as_str() ))?,
    };
}


fn ssh_connect(host_record: &HostRecord, mut sess: &mut Session) -> Result<(), BackupError> {
    //Establish SSH Connection
    let tcp = TcpStream::connect(&host_record.address)?;
    sess.set_tcp_stream(tcp);
    sess.handshake()?;
    //Authenticate
    sess.userauth_password(&host_record.username, &host_record.password)?;
    return Ok(());
}

fn backup_mikrotik_export_host(host_record: &HostRecord) -> Result<(), BackupError> {
    let backup_file_name = make_backup_file_name(host_record);
    let mut sess = Session::new()?;
    ssh_connect(host_record, &mut sess)?;

    //Run Export
    let mut channel = sess.channel_session()?;
    channel.exec("/export show-sensitive verbose")?;
    let mut s = String::new();
    channel.read_to_string(&mut s)?;

    //Write to File
    let file_path = format!("{}/{}","backups",backup_file_name);
    let mut output = File::create(file_path)?;
    write!(output,"{}", s)?;
    return Ok(());
}

fn backup_mikrotik_binary_host(host_record: &HostRecord) -> Result<(), BackupError> {
    let backup_file_name = make_backup_file_name(host_record);
    let mut sess = Session::new()?;
    ssh_connect(host_record, &mut sess)?;

    //Run backup
    let backup_cmd = format!(
        "/system/backup/save name={} dont-encrypt=yes",
        backup_file_name
    );
    let mut channel = sess.channel_session()?;
    channel.exec(&backup_cmd)?;
    let twoSecs = time::Duration::from_millis(2000);
    thread::sleep(twoSecs);

    fetch_backup(&backup_file_name, &sess)?;

    channel = sess.channel_session()?;
    let backup_remove_cmd = format!("/file/remove {}",backup_file_name);
    channel.exec(&backup_remove_cmd)?;
    return Ok(());

}

fn backup_hp_export_host(host_record: &HostRecord) -> Result<(), BackupError> {
    return backup_ssh_to_file(host_record,"screen-length disable\ndisplay current-configuration\n")
}

fn backup_cisco_export_host(host_record: &HostRecord) -> Result<(), BackupError> {
    return backup_ssh_to_file(host_record,"terminal length 0\nsh run\n")
}
fn backup_ssh_to_file(host_record: &HostRecord,cmd: &str) -> Result<(), BackupError> {
    let backup_file_name = make_backup_file_name(host_record);
    let mut sess = Session::new()?;
    //sess.trace(TraceFlags::all());
    ssh_connect(host_record, &mut sess)?;
    //Run Export
    let mut channel = sess.channel_session().unwrap();
    channel.shell().unwrap();
    let mut s = String::new();
    channel.handle_extended_data(ExtendedData::Merge)?;
    channel.write_all(cmd.as_bytes()).unwrap();

    let twoSecs = time::Duration::from_millis(2000);
    thread::sleep(twoSecs*5);
    channel.send_eof().unwrap();
    match channel.read_to_string(&mut s) {
        e => {
            //println!("Received bytes: {}", e?);
            //println!("{}",s);
        } //TODO:: ADDRESS TRANSPORT ERROR?
    }
    channel.wait_eof()?;
    channel.close()?;
    channel.wait_close()?;

    //Write to File
    let file_path = format!("{}/{}","backups",backup_file_name);
    let mut output = File::create(file_path)?;
    write!(output,"{}", s)?;
    return Ok(());
}

fn make_backup_file_name(host_record: &HostRecord) -> String {
    let now: DateTime<Utc> = Utc::now();
    let date_stamp = format!("{}", now.format("%Y%m%d-%H%M"));
    return format!("{}_{}_{}.backup", host_record.name, host_record.method, date_stamp);
}
fn fetch_backup(filename: &str, sess: &Session) -> Result<(), BackupError> {
    let (mut remote_file, _) = sess.scp_recv(Path::new(filename))?;
    let mut contents: Vec<u8> = Vec::new();
    remote_file.read_to_end(&mut contents)?;
    remote_file.send_eof()?;
    remote_file.wait_eof()?;
    remote_file.eof();
    remote_file.close()?;
    remote_file.wait_close()?;

    std::fs::create_dir_all("backups")?;
    let file_path = format!("{}/{}", "backups", filename);
    let mut file = File::create(file_path)?;
    file.write_all(contents.as_slice())?;
    return Ok(());
}
