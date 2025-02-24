# SignalR client for Rust 

[![Current Crates.io Version](https://img.shields.io/crates/v/signalr_client
)](https://crates.io/crates/signalr_client)
[![Documentation](https://img.shields.io/badge/docs-latest-blue)](https://docs.rs/signalr-client/0.1.1/signalr_client/)
![license](https://shields.io/badge/license-MIT%2FApache--2.0-blue)

I made this client because I could not find a client that supported my requirements calling a complex backend written in SignalR from a WASM frontend. This package was designed to overcome this limitation. It supports WebAssembly targets out of the box.

 Read more about SignalR in the [`offical documentation`](https://learn.microsoft.com/en-us/aspnet/core/signalr/introduction?view=aspnetcore-9.0).

I tried to design this package to be simple and convenient to use without writing boilerpart code. Please be free to comment or post issues if you have recommendations or requirements. Contribution is also welcome.

A .NET test backend is also provided for testing purposes (see the /dotnet folder). The tests in the package should run using that backend. Authorization is not tested with the test scenairo, however it's been tested already by using this package in a much larger project of mine. So, it should also work without issues.

## Package Overview
signalr-client is a Rust library designed to provide a way to call SignalR hubs from a Rust cross-platform application. It supports tokio based async runtimes and WASM clients, making it versatile for various environments.

## Installation Instructions
To use this package, add the following to your Cargo.toml:

[dependencies]
signalr-client = "0.1.0"

## Usage Examples

Here is a complex test scenario demonstrating how to use the signalr-client package:

The `SignalRClient` can be used to invoke methods on the hub, send messages, and register callbacks.
The client can be cloned and used freely across different parts of your application.

```
// Connect to the SignalR server with custom configuration
let mut client = SignalRClient::connect_with("localhost", "test", |c| {
    c.with_port(5220); // Set the port to 5220
    c.unsecure(); // Use an unsecure (HTTP) connection
}).await.unwrap();

// Invoke the "SingleEntity" method and assert the result
let re = client.invoke::<TestEntity>("SingleEntity".to_string()).await;
assert!(re.is_ok());

// Unwrap the result and assert the entity's text
let entity = re.unwrap();
assert_eq!(entity.text, "test".to_string());

// Log the entity's details
info!("Entity {}, {}", entity.text, entity.number);

// Enumerate "HundredEntities" and log each entity
let mut he = client.enumerate::<TestEntity>("HundredEntities".to_string()).await;
while let Some(item) = he.next().await {
    info!("Entity {}, {}", item.text, item.number);
}

info!("Finished fetching entities, calling pushes");

// Invoke the "PushEntity" method with arguments and assert the result
let push1 = client.invoke_with_args::<bool, _>("PushEntity".to_string(), |c| {
    c.argument(TestEntity {
        text: "push1".to_string(),
        number: 100,
    });
}).await;
assert!(push1.unwrap());

// Clone the client and invoke the "PushTwoEntities" method with arguments
let mut secondclient = client.clone();
let push2 = secondclient.invoke_with_args::<TestEntity, _>("PushTwoEntities".to_string(), |c| {
    c.argument(TestEntity {
        text: "entity1".to_string(),
        number: 200,
    }).argument(TestEntity {
        text: "entity2".to_string(),
        number: 300,
    });
}).await;
assert!(push2.is_ok());

// Unwrap the result and assert the merged entity's number
let entity = push2.unwrap();
assert_eq!(entity.number, 500);
info!("Merged Entity {}, {}", entity.text, entity.number);

// Drop the second client
drop(secondclient);

// Register callbacks for "callback1" and "callback2"
let c1 = client.register("callback1".to_string(), |ctx| {
    let result = ctx.argument::<TestEntity>(0);
    if result.is_ok() {
        let entity = result.unwrap();
        info!("Callback results entity: {}, {}", entity.text, entity.number);
    }
});

let c2 = client.register("callback2".to_string(), |mut ctx| {
    let result = ctx.argument::<TestEntity>(0);
    if result.is_ok() {
        let entity = result.unwrap();
        info!("Callback2 results entity: {}, {}", entity.text, entity.number);
        let e2 = entity.clone();
        spawn(async move {
            info!("Completing callback2");
            let _ = ctx.complete(e2).await;
        });
    }
});

// Trigger the callbacks
info!("Calling callback1");
_ = client.send_with_args("TriggerEntityCallback".to_string(), |c| {
    c.argument("callback1".to_string());
}).await;

info!("Calling callback2");
let succ = client.invoke_with_args::<bool, _>("TriggerEntityResponse".to_string(), |c| {
    c.argument("callback2".to_string());
}).await;
assert!(succ.unwrap());

// Measure the time taken to fetch a million entities
let now = Instant::now();
{
    let mut me = client.enumerate::<TestEntity>("MillionEntities".to_string()).await;
    while let Some(_) = me.next().await {}
}
let elapsed = now.elapsed();
info!("1 million entities fetched in: {:.2?}", elapsed);

// Unregister the callbacks and disconnect the client
c1.unregister();
c2.unregister();
client.disconnect();
```

## Acknowledgements

Special thanks to the [`maintainer of the signalrs package`](https://github.com/szarykott) for his invaluable inspiration and work in the first SignalR client. Their efforts have significantly contributed to the development of this package.

## Contributing
Contributions are welcome! Please fork the repository and submit pull requests along with an issue or some explanation. Ensure your code follows the existing style and includes tests for any new functionality. 

## License
This project is licensed under the MIT License. See the LICENSE file for more details.
