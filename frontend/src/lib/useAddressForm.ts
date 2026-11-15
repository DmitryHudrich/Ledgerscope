import { useSyncExternalStore } from 'react';

import { getAddressForm, subscribeAddressForm, type AddressForm } from './chain';

export function useAddressForm(): AddressForm {
  return useSyncExternalStore(subscribeAddressForm, getAddressForm, getAddressForm);
}
