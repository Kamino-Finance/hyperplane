/// Send a transaction paid for by the default payer.
///
/// Usage:
/// `send_tx!(ctx, [instruction1, instruction2], signer1, signer2)`
#[macro_export]
macro_rules! send_tx {
    ($ctx:expr, $instr:ident, $($signer:expr),*) => {{
        use solana_sdk::signature::Signer;
        let transaction = ::solana_sdk::transaction::Transaction::new_signed_with_payer(
            &$instr,
            Some(&$ctx.context.payer.pubkey()),
            &[&$ctx.context.payer $(, $signer)*],
            $ctx.context
                .banks_client
                .get_latest_blockhash()
                .await
                .unwrap(),
        );
        $ctx.context
            .banks_client
            .process_transaction_with_commitment(
                transaction,
                ::solana_sdk::commitment_config::CommitmentLevel::Processed,
            )
            .await
    }};
    ($ctx:expr, [$($instr:expr),*], $($signer:expr),*) => {{
        use solana_sdk::signature::Signer;
        let transaction = ::solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[$($instr),*],
            Some(&$ctx.context.payer.pubkey()),
            &[&$ctx.context.payer $(, $signer)*],
            $ctx.context
                .banks_client
                .get_latest_blockhash()
                .await
                .unwrap(),
        );
        $ctx.context
            .banks_client
            .process_transaction_with_commitment(
                transaction,
                ::solana_sdk::commitment_config::CommitmentLevel::Processed,
            )
            .await
    }};
}

#[macro_export]
macro_rules! anchor_error {
    ($err: expr) => {
        solana_sdk::transaction::TransactionError::InstructionError(
            0,
            solana_sdk::instruction::InstructionError::Custom($err as u32),
        )
    };
}

#[macro_export]
macro_rules! hyperplane_error {
    ($err: expr) => {
        ::solana_sdk::transaction::TransactionError::InstructionError(
            0,
            #[allow(clippy::integer_arithmetic)]
            ::solana_sdk::instruction::InstructionError::Custom(6000 + $err as u32),
        )
    };

    ($err: expr, $index: expr) => {
        ::solana_sdk::transaction::TransactionError::InstructionError(
            $index,
            #[allow(clippy::integer_arithmetic)]
            ::solana_sdk::instruction::InstructionError::Custom(6000 + $err as u32),
        )
    };
}

#[macro_export]
macro_rules! token_error {
    ($err: expr) => {
        solana_sdk::transaction::TransactionError::InstructionError(
            0,
            solana_sdk::instruction::InstructionError::Custom($err as u32),
        )
    };
}

#[macro_export]
macro_rules! contextualize_err {
    ($action: ident, $res: ident) => {
        if $res.is_err() {
            println!("action, {:#?} ", $action);
            $res.unwrap()
        } else {
            $res.unwrap()
        }
    };
}
