use futures::StreamExt;
use log::info;
use tokio::{spawn, time::Instant};

use crate::{execution::CallbackHandler, tests::TestEntity, SignalRClient};

#[test_log::test(tokio::test)]
async fn test_service() {
    let mut client = SignalRClient::connect_with("localhost", "test", |c| {
        c.with_port(5220);
        c.unsecure();
    }).await.unwrap();

    let re = client.invoke::<TestEntity>("SingleEntity".to_string()).await;

    assert!(re.is_ok());

    let entity = re.unwrap();
    assert_eq!(entity.text, "test".to_string());    

    info!("Entity {}, {}", entity.text, entity.number);

    let mut he = client.enumerate::<TestEntity>("HundredEntities".to_string()).await;

    while let Some(item) = he.next().await {
        info!("Entity {}, {}", item.text, item.number);
    }

    info!("Finished fetching entities, calling pushes");

    let push1 = client.invoke_with_args::<bool, _>("PushEntity".to_string(), |c| {
        c.argument(TestEntity {
            text: "push1".to_string(),
            number: 100,
        });
    }).await;

    assert!(push1.unwrap());

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
    let entity = push2.unwrap();
    assert_eq!(entity.number, 500);
    info!("Merged Entity {}, {}", entity.text, entity.number);

    drop(secondclient);
    
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

    info!("Calling callback1");

    _ = client.send_with_args("TriggerEntityCallback".to_string(), |c| {
        c.argument("callback1".to_string());
    }).await;

    // info!("Calling callback2");

    // let succ = client.invoke_with_args::<bool, _>("TriggerEntityResponse".to_string(), |c| {
    //     c.argument("callback2".to_string());
    // }).await;

    // assert!(succ.unwrap());

    let now = Instant::now();
    {
        let mut me = client.enumerate::<TestEntity>("MillionEntities".to_string()).await;
        while let Some(_) = me.next().await {}
    }

    let elapsed = now.elapsed();
    info!("1 million entities fetched in: {:.2?}", elapsed);

    c1.unregister();
    c2.unregister();

    client.disconnect();
}