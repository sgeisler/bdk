use bdk::blockchain::electrum::ElectrumBlockchainConfig;
use bdk::blockchain::{
    ConfigurableBlockchain, ElectrumBlockchain, NoopProgress, OfflineBlockchain,
};
use bdk::database::MemoryDatabase;
use bdk::wallet::coin_selection::DumbCoinSelection;
use bdk::wallet::signer::{SignerId, SignerOrdering};
use bdk::{ScriptType, TxBuilder, Wallet};
use bitcoin::util::bip32::{ExtendedPrivKey, ExtendedPubKey};
use bitcoin::{secp256k1, Network};
use miniscript::descriptor::{DescriptorPublicKey, DescriptorXKey};
use miniscript::policy::Concrete as Policy;
use miniscript::Descriptor;
use rand;
use std::io::stdin;
use std::sync::Arc;

fn main() {
    env_logger::init_from_env(
        env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
    );

    let ctx = secp256k1::Secp256k1::new();

    let parties: Vec<ExtendedPrivKey> = [
        "tprv8ZgxMBicQKsPdamjvSwqLBZzwLc973ftcyCse1a4tvaa5PYrde3N67qXYggkVVf7F8LzfdSm6MNNRTAjuUurzdLkJWryzmDPRL5mF5kcGCx",
        "tprv8ZgxMBicQKsPeKhX1HeBaVwikZZh3ekvKM1cGQ8DN9q3YS7zZPPci5kZaHsVcdZ7uxzP97aq91zhhhLigjC2exxyBQ1mSQXYbng4kgCCZNv",
        "tprv8ZgxMBicQKsPeFLkHyBJww4qzNbx82YtxLiy6tbTSwfCZaM8hzSqVDGqb6v3T8SgWAWuWEx54UdbChusEGs5hEf91GfkipHy1KiFeDPLP63"
    ].iter().map(|s| s.parse().unwrap()).collect();

    let policy = Policy::Threshold(
        2,
        parties
            .iter()
            .map(|sk| {
                Policy::Key(DescriptorPublicKey::XPub(DescriptorXKey {
                    source: None,
                    xkey: ExtendedPubKey::from_private(&ctx, sk),
                    derivation_path: [][..].into(),
                    is_wildcard: true,
                }))
            })
            .collect(),
    );
    let descriptor = Descriptor::ShWsh(policy.compile().unwrap());

    let electrum = ElectrumBlockchain::from_config(&ElectrumBlockchainConfig {
        url: "tcp://10.0.0.1:50005".to_string(),
        socks5: None,
    })
    .unwrap();

    let mut wallet = Wallet::new(
        descriptor.clone(),
        None,
        Network::Regtest,
        MemoryDatabase::new(),
        electrum,
    )
    .unwrap();
    wallet.add_signer(
        ScriptType::External,
        SignerId::Fingerprint(parties[0].fingerprint(&ctx)),
        SignerOrdering::default(),
        Arc::new(Box::new(DescriptorXKey {
            source: None,
            xkey: parties[0],
            derivation_path: [][..].into(),
            is_wildcard: true,
        })),
    );

    let addr = wallet.get_new_address().unwrap();

    println!(
        "Please send some regtest BTC to {}, confirm them and press enter",
        addr
    );
    stdin().read_line(&mut String::new()).unwrap();
    wallet.sync(NoopProgress, None);
    println!(
        "We now have {} sats available",
        wallet.get_balance().unwrap()
    );

    let addr2 = wallet.get_new_address().unwrap();
    println!("sending some coins to myself ({})", addr2);
    let (psbt, tx_details) = wallet
        .create_tx(
            TxBuilder::with_recipients(vec![(addr2.script_pubkey(), 42_000)])
                .coin_selection(DumbCoinSelection)
                .enable_rbf(),
        )
        .unwrap();
    println!("Let's see what that is: {:?}", tx_details);

    // Second party signing proicess starts here
    println!("Second signer is signing");
    let mut offline_signer: Wallet<OfflineBlockchain, _> =
        Wallet::new_offline(descriptor, None, Network::Regtest, MemoryDatabase::new()).unwrap();
    offline_signer.add_signer(
        ScriptType::External,
        SignerId::Fingerprint(parties[1].fingerprint(&ctx)),
        SignerOrdering::default(),
        Arc::new(Box::new(DescriptorXKey {
            source: None,
            xkey: parties[1],
            derivation_path: [][..].into(),
            is_wildcard: true,
        })),
    );

    let (psbt, finalized) = offline_signer.sign(psbt, None).unwrap();
    assert!(finalized);
    let tx = psbt.extract_tx();
    println!("Transaction: {:?}", tx);

    // Let's broadcast it!
    wallet.broadcast(tx);
}
