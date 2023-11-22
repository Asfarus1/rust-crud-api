use postgres::error::Error as PostgressError;
use postgres::{Client, NoTls};
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

#[macro_use]
extern crate serde_derive;

//Model: User struct with id, name and email
#[derive(Serialize, Deserialize)]
struct User {
    id: Option<i32>,
    name: String,
    email: String,
}

//DB_URL constant from environment variable
fn db_url()-> String {
     env::var("DB_URL").expect("DB_URL must be set")
}

//Http responces
const HTTP_200_OK: &str = "HTTP/1.1 200 OK\r\nContant-Type: application/json\r\n\r\n";
const HTTP_404_NOT_FOUND: &str = "HTTP/1.1 404 NOT FOUND\r\n\r\n";
const HTTP_500_INTERNAL_SERVER_ERROR: &str = "HTTP/1.1 500 INTERNAL SERVER ERROR\r\n\r\n";

//set up database connection
fn init_db_connection() -> Result<Client, PostgressError> {
    let mut client = Client::connect(&db_url(), NoTls)?;
    client.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            name VARCHAR NOT NULL,
            email VARCHAR NOT NULL
        )",
        &[],
    )?;
    Ok(client)
}

fn main() {
    if let Err(e) = init_db_connection() {
        eprintln!("Error connecting to database: {}", e);
        std::process::exit(1);
    }

    let listener = TcpListener::bind(format!("0.0.0.0:8080")).unwrap();

    //handle incoming requests
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                handle_client(&mut stream);
            }
            Err(e) => {
                eprintln!("Unable to connect: {}", e);
            }
        }
    }
}

fn handle_client(stream: &mut TcpStream) {
    let mut buffer = [0; 1024];
    let mut request = String::new();
    match stream.read(&mut buffer) {
        Ok(size) => {
            request.push_str(String::from_utf8_lossy(&buffer[..size]).as_ref());
        }
        Err(e) => eprintln!("Handle client error: {}", e),
    }
    let (status, message) = match request.split("\r\n").next().unwrap_or_default() {
        path if path.starts_with("GET /users/") => handle_get(&request),
        path if path.starts_with("GET /users") => handle_get_all(),
        path if path.starts_with("POST /users") => handle_post(&request),
        path if path.starts_with("PUT /users") => handle_put(&request),
        path if path.starts_with("DELETE /users/") => handle_delete(&request),
        _ => (
            HTTP_404_NOT_FOUND.to_string(),
            "Resourse not found".to_string(),
        ),
    };

    stream
        .write_all(format!("{}{}", status, message).as_bytes())
        .unwrap();
}

fn handle_get(request: &str) -> (String, String) {
    match (
        get_id_from_request(&request),
        Client::connect(&db_url(), NoTls),
    ) {
        (id, Ok(mut client)) => {
            let r = &client.query_one("SELECT id, name, email FROM users WHERE id = $1", &[&id]);
            match r {
                Ok(row) => {
                    let user = User {
                        id: Some(row.get(0)),
                        name: row.get(1),
                        email: row.get(2),
                    };
                    (
                        HTTP_200_OK.to_string(),
                        serde_json::to_string(&user).unwrap(),
                    )
                }
                _ => (HTTP_404_NOT_FOUND.to_string(), "User not found".to_string()),
            }
        }
        _ => (
            HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
            "Internal server error".to_string(),
        ),
    }
}

fn handle_get_all() -> (String, String) {
    match Client::connect(&db_url(), NoTls) {
        Ok(mut client) => match client.query("SELECT id, name, email FROM users", &[]) {
            Ok(rows) => {
                let users: Vec<User> = rows
                    .iter()
                    .map(|row| User {
                        id: row.get(0),
                        name: row.get(1),
                        email: row.get(2),
                    })
                    .collect();
                (
                    HTTP_200_OK.to_string(),
                    serde_json::to_string(&users).unwrap(),
                )
            }
            _ => (
                HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
                "Internal server error".to_string(),
            ),
        },
        _ => (
            HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
            "Internal server error".to_string(),
        ),
    }
}

fn handle_post(request: &str) -> (String, String) {
    match deserialize_user_from_request(&request) {
        Ok(user) => match Client::connect(&db_url(), NoTls) {
            Ok(mut client) => match client.query_one(
                "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id",
                &[&user.name, &user.email],
            ) {
                Ok(row) => {
                    let id: i32 = row.get(0);
                    (
                        HTTP_200_OK.to_string(),
                        format!("User created with id: {}}}", id),
                    )
                }
                _ => (
                    HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
                    "Internal server error".to_string(),
                ),
            },
            _ => (
                HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
                "Internal server error".to_string(),
            ),
        },
        _ => (
            HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
            "Internal server error".to_string(),
        ),
    }
}

fn handle_put(request: &str) -> (String, String) {
    match (
        get_id_from_request(&request),
        deserialize_user_from_request(&request),
        Client::connect(&db_url(), NoTls),
    ) {
        (id, Ok(user), Ok(mut client)) => {
            match client.execute(
                "UPDATE users SET name = $1, email = $2 WHERE id = $3",
                &[&user.name, &user.email, &id],
            ) {
                Ok(0) => (HTTP_404_NOT_FOUND.to_string(), "User not found".to_string()),
                Ok(_) => (HTTP_200_OK.to_string(), "User updated".to_string()),
                _ => (
                    HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
                    "Internal server error".to_string(),
                ),
            }
        }
        _ => (
            HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
            "Internal server error".to_string(),
        ),
    }
}

fn handle_delete(request: &str) -> (String, String) {
    match (
        get_id_from_request(&request),
        Client::connect(&db_url(), NoTls),
    ) {
        (id, Ok(mut client)) => match client.execute("DELETE FROM users WHERE id = $1", &[&id]) {
            Ok(0) => (HTTP_404_NOT_FOUND.to_string(), "User not found".to_string()),
            Ok(_) => (HTTP_200_OK.to_string(), "User deleted".to_string()),
            _ => (
                HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
                "Internal server error".to_string(),
            ),
        },
        _ => (
            HTTP_500_INTERNAL_SERVER_ERROR.to_string(),
            "Internal server error".to_string(),
        ),
    }
}

//get if from request function
fn get_id_from_request(request: &str) -> i32 {
    request
        .split("/")
        .nth(2)
        .unwrap_or_default()
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .parse().unwrap_or_default()
}

fn deserialize_user_from_request(request: &str) -> Result<User, serde_json::Error> {
    serde_json::from_str(request.split("\r\n\r\n").last().unwrap_or_default())
}
