use clap::{value_parser, Arg, ArgAction, Command};

extern crate internal_developer_portal;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let matches = Command::new("internal-developer-portal")
        .about("Internal Developer Portal commands")
        .arg_required_else_help(true)
        .subcommand(
            Command::new("users")
                .about("User management")
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("create")
                        .about("Create a new user")
                        .arg_required_else_help(true)
                        .arg(Arg::new("username").required(true))
                        .arg(Arg::new("password").required(true))
                        .arg(
                            Arg::new("roles")
                                .required(true)
                                .num_args(1..)
                                .value_delimiter(','),
                        ),
                )
                .subcommand(
                    Command::new("ensure-admin")
                        .about("Ensure a seed admin user exists")
                        .arg(Arg::new("username").long("username"))
                        .arg(Arg::new("password").long("password"))
                        .arg(
                            Arg::new("roles")
                                .long("roles")
                                .num_args(1..)
                                .value_delimiter(','),
                        )
                        .arg(
                            Arg::new("reset-password")
                                .long("reset-password")
                                .action(ArgAction::SetTrue),
                        ),
                )
                .subcommand(Command::new("list").about("List existing users"))
                .subcommand(
                    Command::new("delete")
                        .about("Delete user by ID")
                        .arg_required_else_help(true)
                        .arg(
                            Arg::new("id")
                                .required(true)
                                .value_parser(value_parser!(i32)),
                        ),
                ),
        )
        .get_matches();

    if let Some(("users", sub_matches)) = matches.subcommand() {
        match sub_matches.subcommand() {
            Some(("create", sub_matches)) => {
                internal_developer_portal::commands::create_user(
                    sub_matches
                        .get_one::<String>("username")
                        .unwrap()
                        .to_owned(),
                    sub_matches
                        .get_one::<String>("password")
                        .unwrap()
                        .to_owned(),
                    sub_matches
                        .get_many::<String>("roles")
                        .unwrap()
                        .map(|v| v.to_owned())
                        .collect(),
                )
                .await
            }
            Some(("ensure-admin", sub_matches)) => {
                let username = sub_matches
                    .get_one::<String>("username")
                    .cloned()
                    .or_else(|| std::env::var("SEED_ADMIN_USERNAME").ok())
                    .unwrap_or_else(|| "admin".to_owned());
                let password = sub_matches
                    .get_one::<String>("password")
                    .cloned()
                    .or_else(|| std::env::var("SEED_ADMIN_PASSWORD").ok())
                    .unwrap_or_else(|| "admin123".to_owned());
                let roles = sub_matches
                    .get_many::<String>("roles")
                    .map(|roles| roles.map(ToOwned::to_owned).collect())
                    .or_else(|| {
                        std::env::var("SEED_ADMIN_ROLES")
                            .ok()
                            .map(|roles| vec![roles])
                    })
                    .unwrap_or_else(|| vec!["admin".to_owned(), "member".to_owned()]);
                let reset_password = sub_matches.get_flag("reset-password")
                    || std::env::var("SEED_ADMIN_RESET_PASSWORD").as_deref() == Ok("true");

                internal_developer_portal::commands::ensure_admin_user(
                    username,
                    password,
                    roles,
                    reset_password,
                )
                .await
            }
            Some(("list", _)) => internal_developer_portal::commands::list_users().await,
            Some(("delete", sub_matches)) => {
                internal_developer_portal::commands::delete_user(
                    sub_matches.get_one::<i32>("id").unwrap().to_owned(),
                )
                .await
            }
            _ => {}
        }
    }
}
