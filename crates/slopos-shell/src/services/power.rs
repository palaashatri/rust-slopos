use slopos_services::power::PowerClient;
use std::io;

pub fn request_suspend<F>(completion: F) -> Result<(), io::Error>
where
    F: FnOnce(Result<(), String>) + Send + 'static,
{
    std::thread::Builder::new()
        .name("slopos-suspend-request".to_owned())
        .spawn(move || {
            let result = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(async {
                    let client = PowerClient::connect()
                        .await
                        .map_err(|error| error.to_string())?;
                    client
                        .request_suspend()
                        .await
                        .map_err(|error| error.to_string())
                }),
                Err(error) => Err(error.to_string()),
            };
            glib::MainContext::default().invoke(move || completion(result));
        })
        .map(|_| ())
}
