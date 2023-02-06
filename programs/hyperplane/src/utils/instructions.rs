use std::cell::Ref;

use anchor_lang::__private::bytemuck;
use anchor_lang::prelude::error::ErrorCode as AnchorError;
use anchor_lang::prelude::*;
use anchor_lang::AccountDeserialize;
use anchor_lang::Discriminator;
use anchor_lang::Key;

pub fn deserialize<T: AccountDeserialize + Discriminator>(account: &AccountInfo<'_>) -> Result<T> {
    let data = account.clone().data.borrow().to_owned();
    if data.len() < T::discriminator().len() {
        return Err(ErrorCode::AccountDiscriminatorNotFound.into());
    }
    let discriminator = &data[..8];
    if discriminator != T::discriminator() {
        msg!(
            "Expected discriminator for account {:?} ({:?}) is different from received {:?}",
            account.key(),
            T::discriminator(),
            discriminator
        );
        return err!(AnchorError::AccountDiscriminatorMismatch);
    }

    let mut data: &[u8] = &data;
    let user: T = T::try_deserialize(&mut data)?;

    Ok(user)
}

pub fn zero_copy_deserialize<'info, T: bytemuck::AnyBitPattern + Discriminator>(
    account: &'info AccountInfo,
) -> Result<Ref<'info, T>> {
    let data = account.data.try_borrow().unwrap();

    let disc_bytes = data.get(..8).ok_or_else(|| {
        msg!(
            "Account {:?} does not have enough bytes to be deserialized",
            account.key()
        );
        AnchorError::AccountDidNotDeserialize
    })?;
    if disc_bytes != T::discriminator() {
        msg!(
            "Expected discriminator for account {:?} ({:?}) is different from received {:?}",
            account.key(),
            T::discriminator(),
            disc_bytes
        );
        return Err(AnchorError::AccountDiscriminatorMismatch.into());
    }

    Ok(Ref::map(data, |data| bytemuck::from_bytes(&data[8..])))
}
