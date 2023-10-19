#![no_std]

use gstd::{collections::HashMap, exec, msg, prelude::*, ActorId};
use kee_bee_io::{FBSEvent, InitConfig};

pub mod utils;

static mut KEE_BEE_SHARE: Option<KeeBeeShare> = None;
const ETH1: u128 = 10 ^ 18;

#[derive(Debug, Default)]
pub struct KeeBeeShare {
    pub shares_balance: HashMap<ActorId, HashMap<ActorId, u128>>,
    pub share_supply: HashMap<ActorId, u128>,
    pub manager: HashMap<ActorId, bool>,
    pub protocol_fee_destination: ActorId,
    pub protocol_fee_percent: u128,
    pub subject_fee_percent: u128,
    pub max_fee_percent: u128,
    pub max_amount: u8,
}

#[no_mangle]
extern "C" fn init() {
    let init_config: InitConfig = msg::load().expect("Unable to decode protocoal fee destination");
    let mut kee_bee_share = KeeBeeShare {
        protocol_fee_destination: init_config.protocol_fee_destination,
        protocol_fee_percent: init_config.protocol_fee_percent,
        subject_fee_percent: init_config.subject_fee_percent,
        max_fee_percent: 100000000000000000u128,
        max_amount: 1,
        ..Default::default()
    };
    kee_bee_share.manager.insert(msg::source(), true);
    unsafe {
        KEE_BEE_SHARE = Some(kee_bee_share);
    }
    // isManager[msg.sender] = true;
    // protocolFeeDestination = _protocolFeeDestination;
}

// event Trade(address trader, address subject, bool isBuy, uint256 shareAmount, uint256 ethAmount, uint256 protocolEthAmount, uint256 subjectEthAmount, uint256 supply);

impl KeeBeeShare {
    fn get_price(&self, supply: u128, amount: u128) -> u128 {
        assert!(amount <= self.max_amount.into(), "amount too high");
        let sum1 = if supply == 0 {
            0
        } else {
            (supply - 1) * (supply) * (2 * (supply - 1) + 1) / 6
        };
        let sum2 = if supply == 0 && amount == 1 {
            0
        } else {
            (supply - 1 + amount) * (supply + amount) * (2 * (supply - 1 + amount) + 1) / 6
        };
        let summation = sum2 - sum1;
        return summation * ETH1 / 16000u128;
    }

    pub fn get_buy_price(&self, shares_subject: ActorId, amount: u128) -> u128 {
        return self.get_price(
            *self.share_supply.get(&shares_subject).unwrap_or(&0u128),
            amount,
        );
    }

    pub fn get_sell_price(&self, shares_subject: ActorId, amount: u128) -> u128 {
        return self.get_price(
            *self.share_supply.get(&shares_subject).unwrap_or(&0u128) - amount,
            amount,
        );
    }

    pub fn get_buy_price_after_fee(&self, shares_subject: ActorId, amount: u128) -> u128 {
        let price = self.get_buy_price(shares_subject, amount);
        let protocol_fee = price * self.protocol_fee_percent / ETH1;
        let subject_fee = price * self.subject_fee_percent / ETH1;
        return price + protocol_fee + subject_fee;
    }

    pub fn buy_shares(&mut self, shares_subject: ActorId, amount: u128) {
        let supply = self.share_supply.get(&shares_subject).unwrap_or(&0).clone();
        let trader = msg::source();
        assert!(
            supply > 0 || shares_subject == trader,
            "Only the shares' subject can buy the first share"
        );
        let price = self.get_price(supply, amount);
        let protocol_fee = price * self.protocol_fee_percent / ETH1;
        let subject_fee = price * self.subject_fee_percent / ETH1;
        assert!(
            msg::value() >= price + protocol_fee + subject_fee,
            "Insufficient payment"
        );
        self.shares_balance
            .entry(shares_subject)
            .or_insert(Default::default())
            .entry(trader)
            .and_modify(|share_balance| *share_balance += amount)
            .or_insert(amount);
        // sharesSupply[sharesSubject] = supply + amount;
        self.share_supply
            .entry(shares_subject)
            .and_modify(|supply| *supply += amount)
            .or_insert(amount);
        msg::send(self.protocol_fee_destination, "", protocol_fee).expect("send ptotocal fee fail");
        msg::send(shares_subject, "", subject_fee).expect("send subject fee fail");

        // Trade(msg.sender, sharesSubject, true, amount, price, protocolFee, subjectFee, supply + amount);
        let trade = FBSEvent::Trade {
            trader,
            subject: shares_subject,
            is_buy: true,
            share_amount: amount,
            eth_amount: price,
            protocol_eth_amount: protocol_fee,
            subject_eth_amount: subject_fee,
            supply: supply + amount,
        };
        msg::reply(trade, 0).unwrap();
    }

    pub fn sell_shares(&mut self, shares_subject: ActorId, amount: u128) {
        let supply = self.share_supply.get(&shares_subject).unwrap_or(&0).clone();
        let trader = msg::source();
        assert!(supply > amount, "Cannot sell the last share");
        let price = self.get_price(supply - amount, amount);
        let protocol_fee = price * self.protocol_fee_percent / ETH1;
        let subject_fee = price * self.subject_fee_percent / ETH1;
        let share_balance = self
            .shares_balance
            .get(&shares_subject)
            .unwrap()
            .get(&trader)
            .unwrap()
            .clone();
        assert!(share_balance >= amount, "Insufficient shares");
        self.shares_balance
            .get_mut(&shares_subject)
            .unwrap()
            .entry(trader)
            .and_modify(|share| *share -= amount);
        self.share_supply
            .entry(shares_subject)
            .and_modify(|supply| *supply -= amount);

        msg::send(trader, "", price - protocol_fee - subject_fee).unwrap();
        msg::send(self.protocol_fee_destination, "", protocol_fee).unwrap();
        msg::send(shares_subject, "", subject_fee).expect("send subject fee fail");
        let trade = FBSEvent::Trade {
            trader,
            subject: shares_subject,
            is_buy: false,
            share_amount: amount,
            eth_amount: price,
            protocol_eth_amount: protocol_fee,
            subject_eth_amount: subject_fee,
            supply: supply + amount,
        };
        msg::reply(trade, 0).unwrap();
    }

    pub fn set_max_amount(&mut self,max_amount:u8){
        self.assert_admin();
        self.max_amount = max_amount;
    }

    // function setFeeDestination(address _feeDestination) public onlyManager {
    //     protocolFeeDestination = _feeDestination;
    // }

    // function setProtocolFeePercent(uint256 _feePercent) public onlyManager {
    //     protocolFeePercent = _feePercent;
    // }

    // function setSubjectFeePercent(uint256 _feePercent) public onlyManager {
    //     subjectFeePercent = _feePercent;
    // }
}
