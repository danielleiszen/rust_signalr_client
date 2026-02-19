use futures::StreamExt;
use log::info;
use tokio::time::Instant;

use crate::{SignalRClient, tests::TestEntity};

#[test_log::test(tokio::test)]
async fn test_msgpack_service() {
    let mut client = SignalRClient::connect_with("localhost", "test", |c| {
        c.with_port(5220);
        c.unsecure();
        c.with_messagepack_protocol();
    }).await.unwrap();

    // Test invoke (single entity)
    let re = client.invoke::<TestEntity>("SingleEntity".to_string()).await;
    assert!(re.is_ok());
    let entity = re.unwrap();
    assert_eq!(entity.text, "test".to_string());
    info!("MsgPack Entity {}, {}", entity.text, entity.number);

    // Test streaming (hundred entities)
    let mut he = client.enumerate::<TestEntity>("HundredEntities".to_string()).await;
    let mut count = 0;
    while let Some(item) = he.next().await {
        info!("MsgPack Stream Entity {}, {}", item.text, item.number);
        count += 1;
    }
    assert_eq!(count, 100);
    info!("MsgPack: Finished fetching {} entities", count);

    // Test invoke with arguments
    let push1 = client.invoke_with_args::<bool, _>("PushEntity".to_string(), |c| {
        c.argument(TestEntity {
            text: "push1".to_string(),
            number: 100,
        });
    }).await;
    assert!(push1.unwrap());

    // Test invoke with multiple arguments
    let push2 = client.invoke_with_args::<TestEntity, _>("PushTwoEntities".to_string(), |c| {
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
    info!("MsgPack Merged Entity {}, {}", entity.text, entity.number);

    // Test large stream performance
    let now = Instant::now();
    {
        let mut me = client.enumerate::<TestEntity>("MillionEntities".to_string()).await;
        while let Some(_) = me.next().await {}
    }
    let elapsed = now.elapsed();
    info!("MsgPack: 1 million entities fetched in: {:.2?}", elapsed);

    client.disconnect();
}
