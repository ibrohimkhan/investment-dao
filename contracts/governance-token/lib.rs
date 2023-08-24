#![cfg_attr(not(feature = "std"), no_std, no_main)]
#![feature(min_specialization)]

pub use self::governance_token::GovernanceTokenRef;

#[openbrush::implementation(PSP22, PSP22Metadata)]
#[openbrush::contract]
mod governance_token {

    use ink::storage::Mapping;
    use openbrush::traits::Storage;

    #[ink(storage)]
    #[derive(Default, Storage)]
    pub struct GovernanceToken {
        #[storage_field]
        psp22: psp22::Data,

        #[storage_field]
        metadata: metadata::Data,

        balances: Mapping<AccountId, Balance>,

        total_supply: Balance,

        circulating_supply: Balance,
    }

    impl GovernanceToken {
        #[ink(constructor)]
        pub fn new(
            initial_supply: Balance,
            name: Option<String>,
            symbol: Option<String>,
            decimal: u8,
        ) -> Self {
            let mut _instance = Self::default();

            psp22::Internal::_mint_to(
                &mut _instance,
                Self::env().caller(),
                initial_supply,
            )
            .expect("Should mint");

            _instance.metadata.name.set(&name);
            _instance.metadata.symbol.set(&symbol);
            _instance.metadata.decimals.set(&decimal);

            _instance.balances = Mapping::default();
            _instance.total_supply = initial_supply;
            _instance.circulating_supply = 0;

            _instance
        }

        // A way to drop some tokens to users for voting
        #[ink(message)]
        pub fn transfer_to(&mut self, recipient: AccountId, amount: Balance) {
            if amount + self.circulating_supply < self.total_supply {
                let recipient_balance = self.balance_of(recipient);

                self.balances
                    .insert(recipient, &(recipient_balance + amount));
                self.circulating_supply += amount;
            }
        }

        #[ink(message)]
        pub fn weight(&self, account: AccountId) -> u64 {
            let balance = self.balances.get(account).unwrap_or_default();
            (balance * 100 / self.total_supply) as u64
        }

        #[ink(message)]
        pub fn balance_of(&self, account: AccountId) -> Balance {
            self.balances.get(account).unwrap_or_default()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn default_accounts() -> ink::env::test::DefaultAccounts<Environment> {
            ink::env::test::default_accounts::<Environment>()
        }

        fn alice() -> AccountId {
            default_accounts().alice
        }

        #[ink::test]
        fn new_works() {
            let contract = GovernanceToken::new(
                1000,
                Some("VoteCoin".into()),
                Some("VCT".into()),
                8,
            );
            assert_eq!(contract.total_supply, 1000);
            assert_eq!(contract.circulating_supply, 0);
        }

        #[ink::test]
        fn transfer_to_works() {
            let mut contract = GovernanceToken::new(
                1000,
                Some("VoteCoin".into()),
                Some("VCT".into()),
                8,
            );
            assert_eq!(contract.total_supply, 1000);

            contract.transfer_to(alice(), 10);
            assert_eq!(contract.circulating_supply, 10);
            assert_eq!(contract.balance_of(alice()), 10);
        }

        #[ink::test]
        fn weight_works() {
            let mut contract =
                GovernanceToken::new(100, Some("VoteCoin".into()), Some("VCT".into()), 8);
            assert_eq!(contract.total_supply, 100);

            contract.transfer_to(alice(), 3);
            assert_eq!(contract.weight(alice()), 3);
        }
    }
}
