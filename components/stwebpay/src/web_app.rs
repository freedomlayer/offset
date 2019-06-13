use tide::{Context, EndpointResult, error::ResultExt}; /*, response, App};*/

async fn payment(mut cx: Context<()>) -> EndpointResult<String> {
    let msg: String = await!(cx.body_json()).client_err()?;
    println!("msg = {:?}", msg);

    /*
    let mut messages = cx.app_data().messages();
    let id = messages.len();
    messages.push(msg);

    Ok(id.to_string())
    */
    Ok(msg)
}

async fn check(mut _cx: Context<()>) -> EndpointResult<String> {
    /*
    let mut messages = cx.app_data().messages();
    let id = messages.len();
    messages.push(msg);

    Ok(id.to_string())
    */
    println!("Was in check!");
    Ok("Hello world".to_string())
}

pub fn serve_app() -> Result<(), std::io::Error> {
    let mut app = tide::App::new(());
    app.at("/payment").post(payment);
    app.at("/check").get(check);
    app.serve("127.0.0.1:10000")
}

