import React, { useEffect, useState } from 'react';
import { Box, Typography } from '@mui/material';
import { accounts } from './mocks';

const fetchMnemonic = (accountName: string): Promise<string> =>
  new Promise((res) => {
    const account = accounts.find((acc) => acc.name === accountName);
    if (account) setTimeout(() => res(account.mnemonic), 0);
    else res('n/a');
  });

export const ShowMnemonic = ({ accountName }: { accountName: string }) => {
  const [showMnemonic, setShowMnemonic] = useState<string>();
  const [mnemonic, setMnemonic] = useState<string>();

  useEffect(() => {
    const getMnemonic = async () => {
      const mnic = await fetchMnemonic(accountName);
      setMnemonic(mnic);
    };

    if (showMnemonic) getMnemonic();
    else setMnemonic(undefined);
  }, [showMnemonic]);

  return (
    <Box>
      <Typography
        variant="body2"
        sx={{ textDecoration: 'underline' }}
        onClick={(e) => {
          e.stopPropagation();
          setShowMnemonic((show) => (!show ? accountName : undefined));
        }}
      >
        {`${showMnemonic ? 'Hide' : 'Show'} mnemonic`}
      </Typography>
      {mnemonic && <Typography variant="caption">{mnemonic}</Typography>}
    </Box>
  );
};
