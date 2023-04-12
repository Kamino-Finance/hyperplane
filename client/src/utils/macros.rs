#[macro_export]
macro_rules! send_tx {
    ($client:expr, $tx_builder:expr, [$($signers:expr),*]) => {
        if $client.config.multisig {
            let signers: Vec<&::anchor_client::solana_sdk::signature::Keypair> = vec![$($signers),*];
            // send the transaction immediately if there are signers required
            if signers.len() > 0 {
                if $client.config.dry_run {
                    let tx = $tx_builder.build(&[$($signers),*]).await?;
                    let res = $client
                        .get_rpc()
                        .simulate_transaction(&tx)
                        .await?;
                    ::tracing::info!("Simulated transaction: {:?}", tx);
                    ::tracing::info!("Result: {:?}", res);
                } else {
                    let sig = $client
                        .client
                        .send_and_confirm_transaction($tx_builder.build(&[$($signers),*]).await?)
                        .await?;
                         ::tracing::info!("Transaction sent: {:?}", sig);
                }
            } else {
                ::tracing::info!("Base64 encoded transaction:\n\n{:?}\n", $tx_builder.to_base64());
                ::tracing::info!("Base58 encoded transaction:\n\n{:?}\n", $tx_builder.to_base58());
            }
        } else if $client.config.dry_run {
            ::tracing::info!(
                "Base64 encoded transaction:\n\n{:?}\n",
                $tx_builder.to_base64(),
            );
            ::tracing::info!(
                "Base64 encoded transaction:\n\n{:?}\n",
                $tx_builder.to_base58(),
            );
            let tx = $tx_builder.build(&[$($signers),*]).await?;
            let res = $client
                .get_rpc()
                .simulate_transaction(&tx)
                .await?;
            ::tracing::info!("Simulated transaction: {:?}", tx);
            ::tracing::info!("Result: {:?}", res);
        } else {
            let sig = $client
                .client
                .send_and_confirm_transaction($tx_builder.build(&[$($signers),*]).await?)
                .await?;
            ::tracing::info!("Transaction sent: {:?}", sig);
        }
    };
}
