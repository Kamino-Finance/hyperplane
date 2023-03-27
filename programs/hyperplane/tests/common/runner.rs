use anchor_lang::prelude::Clock;
use solana_program_test::{processor, ProgramTest};

use super::types::TestContext;
use crate::common::fixtures::ProgramDependency;

pub fn program(dependencies: &[ProgramDependency]) -> ProgramTest {
    let program_test =
        ProgramTest::new("hyperplane", hyperplane::ID, processor!(hyperplane::entry));

    dependencies
        .iter()
        .for_each(|_dep| unimplemented!("No dependency supported yet."));
    program_test
}

pub async fn start(test: ProgramTest) -> TestContext {
    let mut context = test.start_with_context().await;
    let rent = context.banks_client.get_rent().await.unwrap();

    TestContext { context, rent }
}

pub async fn warp_two_slots(ctx: &mut TestContext) {
    let clock = get_clock(ctx).await;
    ctx.context.warp_to_slot(clock.slot + 2).unwrap();
}

pub async fn get_clock(ctx: &mut TestContext) -> Clock {
    ctx.context
        .banks_client
        .get_sysvar::<Clock>()
        .await
        .unwrap()
}
